use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::history::{History, HistoryPolicies};
use crate::item::HistoryItem;

pub const FORMAT_MAGIC: &[u8] = b"COSMIC_PASTE_HISTORY\0";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("invalid history name: {0}")]
    InvalidHistoryName(String),
    #[error("corrupted history file {path}: {reason}")]
    Corrupted { path: PathBuf, reason: String },
    #[error("unsupported history format version {found} (max supported {supported})")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    RonDecode(#[from] ron::error::SpannedError),
    #[error(transparent)]
    RonEncode(#[from] ron::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type PersistenceResult<T> = std::result::Result<T, PersistenceError>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryFile {
    pub version: u32,
    pub name: String,
    pub policies: HistoryPolicies,
    pub items: Vec<HistoryItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub active_index: usize,
    pub current_history: String,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            active_index: 0,
            current_history: "history".to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum LoadHistoryOutcome {
    Loaded(History),
    RecoveredFromBackup(History),
    EmptyAfterCorruption { name: String },
}

/// Resolved on-disk locations under the cosmic-paste data root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataPaths {
    base: PathBuf,
}

impl DataPaths {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn default_xdg() -> PersistenceResult<Self> {
        let base = dirs::data_dir()
            .ok_or_else(|| PersistenceError::Io(std::io::Error::other("no data directory")))?
            .join("cosmic-paste");
        Ok(Self::new(base))
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn histories_dir(&self) -> PathBuf {
        self.base.join("histories")
    }

    pub fn history_file(&self, name: &str) -> PersistenceResult<PathBuf> {
        validate_history_name(name)?;
        Ok(self.histories_dir().join(format!("{name}.ron")))
    }

    pub fn history_backup_file(&self, name: &str) -> PersistenceResult<PathBuf> {
        validate_history_name(name)?;
        Ok(self.histories_dir().join(format!("{name}.ron.bak")))
    }

    pub fn history_blob_dir(&self, name: &str) -> PersistenceResult<PathBuf> {
        validate_history_name(name)?;
        Ok(self.histories_dir().join(format!("{name}.blobs")))
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.base.join("backups")
    }

    pub fn state_file(&self) -> PathBuf {
        self.base.join("state.json")
    }
}

pub struct HistoryStore {
    paths: DataPaths,
}

impl HistoryStore {
    pub fn new(paths: DataPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    pub fn ensure_dirs(&self) -> PersistenceResult<()> {
        fs::create_dir_all(self.paths.histories_dir())?;
        fs::create_dir_all(self.paths.backups_dir())?;
        Ok(())
    }

    pub fn save_history(&self, history: &History) -> PersistenceResult<()> {
        self.ensure_dirs()?;
        let name = history.name();
        let path = self.paths.history_file(name)?;

        let file = HistoryFile {
            version: FORMAT_VERSION,
            name: name.to_owned(),
            policies: history.policies().clone(),
            items: history.items().to_vec(),
        };

        let payload = encode_history_file(&file)?;
        atomic_write(&path, &payload)?;

        let backup = self.paths.history_backup_file(name)?;
        fs::copy(&path, &backup)?;
        Ok(())
    }

    pub fn load_history(
        &self,
        name: &str,
        default_policies: HistoryPolicies,
    ) -> PersistenceResult<LoadHistoryOutcome> {
        self.ensure_dirs()?;
        let path = self.paths.history_file(name)?;
        let backup = self.paths.history_backup_file(name)?;

        if !path.exists() && !backup.exists() {
            return Ok(LoadHistoryOutcome::Loaded(History::new(
                name,
                default_policies,
            )));
        }

        match read_history_file(&path) {
            Ok(file) => Ok(LoadHistoryOutcome::Loaded(history_from_file(file))),
            Err(_primary_err) => match read_history_file(&backup) {
                Ok(file) => Ok(LoadHistoryOutcome::RecoveredFromBackup(history_from_file(
                    file,
                ))),
                Err(_) => {
                    if path.exists() {
                        let corrupt = path.with_extension("ron.corrupt");
                        let _ = fs::rename(&path, corrupt);
                    }
                    Ok(LoadHistoryOutcome::EmptyAfterCorruption {
                        name: name.to_owned(),
                    })
                }
            },
        }
    }

    pub fn save_blob(
        &self,
        history_name: &str,
        checksum: &[u8; 32],
        data: &[u8],
    ) -> PersistenceResult<PathBuf> {
        self.ensure_dirs()?;
        let dir = self.paths.history_blob_dir(history_name)?;
        fs::create_dir_all(&dir)?;
        let path = dir.join(checksum_hex(checksum));
        atomic_write(&path, data)?;
        Ok(path)
    }

    pub fn load_blob(
        &self,
        history_name: &str,
        checksum: &[u8; 32],
    ) -> PersistenceResult<Vec<u8>> {
        let path = self
            .paths
            .history_blob_dir(history_name)?
            .join(checksum_hex(checksum));
        Ok(fs::read(path)?)
    }

    pub fn save_session_state(&self, state: &SessionState) -> PersistenceResult<()> {
        self.ensure_dirs()?;
        let payload = serde_json::to_vec_pretty(state)?;
        atomic_write(&self.paths.state_file(), &payload)?;
        Ok(())
    }

    pub fn load_session_state(&self) -> PersistenceResult<SessionState> {
        let path = self.paths.state_file();
        if !path.exists() {
            return Ok(SessionState::default());
        }
        let payload = fs::read(&path)?;
        Ok(serde_json::from_slice(&payload)?)
    }
}

fn history_from_file(file: HistoryFile) -> History {
    History::from_persisted(file.name, file.policies, file.items)
}

fn validate_history_name(name: &str) -> PersistenceResult<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(PersistenceError::InvalidHistoryName(name.to_owned()))
    }
}

fn encode_history_file(file: &HistoryFile) -> PersistenceResult<Vec<u8>> {
    let mut out = Vec::with_capacity(FORMAT_MAGIC.len() + 4 + 256);
    out.extend_from_slice(FORMAT_MAGIC);
    out.extend_from_slice(&file.version.to_le_bytes());
    let ron_body = ron::ser::to_string_pretty(file, ron::ser::PrettyConfig::new())?;
    out.extend_from_slice(ron_body.as_bytes());
    Ok(out)
}

fn read_history_file(path: &Path) -> PersistenceResult<HistoryFile> {
    let bytes = fs::read(path)?;
    decode_history_file(&bytes, path)
}

fn decode_history_file(bytes: &[u8], path: &Path) -> PersistenceResult<HistoryFile> {
    if bytes.len() < FORMAT_MAGIC.len() + 4 {
        return Err(PersistenceError::Corrupted {
            path: path.to_path_buf(),
            reason: "file too short".to_owned(),
        });
    }

    if &bytes[..FORMAT_MAGIC.len()] != FORMAT_MAGIC {
        return Err(PersistenceError::Corrupted {
            path: path.to_path_buf(),
            reason: "bad magic header".to_owned(),
        });
    }

    let version = u32::from_le_bytes(bytes[FORMAT_MAGIC.len()..FORMAT_MAGIC.len() + 4].try_into().unwrap());
    if version > FORMAT_VERSION {
        return Err(PersistenceError::UnsupportedVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }

    let ron_body = std::str::from_utf8(&bytes[FORMAT_MAGIC.len() + 4..]).map_err(|err| {
        PersistenceError::Corrupted {
            path: path.to_path_buf(),
            reason: format!("invalid utf-8 in ron body: {err}"),
        }
    })?;

    let file: HistoryFile = ron::from_str(ron_body)?;
    Ok(file)
}

fn atomic_write(path: &Path, payload: &[u8]) -> PersistenceResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension(format!(
        "tmp.{}",
        std::process::id()
    ));

    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(payload)?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn checksum_hex(checksum: &[u8; 32]) -> String {
    checksum.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::RichPayload;

    fn temp_store() -> (tempfile::TempDir, HistoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(DataPaths::new(dir.path().to_path_buf()));
        (dir, store)
    }

    #[test]
    fn roundtrip_save_and_load() {
        let (_dir, store) = temp_store();
        let mut history = History::with_defaults("history");
        history.ingest_text("hello", None, 1);
        history.ingest_text("world", None, 2);

        store.save_history(&history).unwrap();
        let loaded = match store.load_history("history", HistoryPolicies::default()).unwrap() {
            LoadHistoryOutcome::Loaded(h) => h,
            other => panic!("expected Loaded, got {other:?}"),
        };

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0).unwrap().plain_text(), Some("world"));
        assert_eq!(loaded.get(1).unwrap().plain_text(), Some("hello"));
    }

    #[test]
    fn recovers_from_backup_when_primary_corrupt() {
        let (_dir, store) = temp_store();
        let mut history = History::with_defaults("history");
        history.ingest_text("backup-me", None, 1);
        store.save_history(&history).unwrap();

        let path = store.paths().history_file("history").unwrap();
        fs::write(&path, b"not a history file").unwrap();

        let loaded = match store.load_history("history", HistoryPolicies::default()).unwrap() {
            LoadHistoryOutcome::RecoveredFromBackup(h) => h,
            other => panic!("expected RecoveredFromBackup, got {other:?}"),
        };
        assert_eq!(loaded.get(0).unwrap().plain_text(), Some("backup-me"));
    }

    #[test]
    fn empty_history_when_both_files_corrupt() {
        let (_dir, store) = temp_store();
        let mut history = History::with_defaults("history");
        history.ingest_text("gone", None, 1);
        store.save_history(&history).unwrap();

        let path = store.paths().history_file("history").unwrap();
        let backup = store.paths().history_backup_file("history").unwrap();
        fs::write(&path, b"bad").unwrap();
        fs::write(&backup, b"also bad").unwrap();

        let outcome = store
            .load_history("history", HistoryPolicies::default())
            .unwrap();
        assert!(matches!(
            outcome,
            LoadHistoryOutcome::EmptyAfterCorruption { .. }
        ));
    }

    #[test]
    fn rejects_invalid_history_names() {
        let (_dir, store) = temp_store();
        let err = store.paths().history_file("../escape").unwrap_err();
        assert!(matches!(err, PersistenceError::InvalidHistoryName(_)));
    }

    #[test]
    fn blob_roundtrip() {
        let (_dir, store) = temp_store();
        let checksum = [7u8; 32];
        let data = b"png-bytes";
        store.save_blob("history", &checksum, data).unwrap();
        let loaded = store.load_blob("history", &checksum).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn session_state_roundtrip() {
        let (_dir, store) = temp_store();
        let state = SessionState {
            active_index: 3,
            current_history: "work".to_owned(),
        };
        store.save_session_state(&state).unwrap();
        let loaded = store.load_session_state().unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn encoded_file_has_magic_header() {
        let file = HistoryFile {
            version: FORMAT_VERSION,
            name: "history".to_owned(),
            policies: HistoryPolicies::default(),
            items: vec![],
        };
        let encoded = encode_history_file(&file).unwrap();
        assert!(encoded.starts_with(FORMAT_MAGIC));
    }

    #[test]
    fn persists_rich_text_payload() {
        let (_dir, store) = temp_store();
        let mut history = History::with_defaults("history");
        history.ingest_text(
            "rich",
            Some(RichPayload {
                html: Some("<b>rich</b>".to_owned()),
                xml: None,
            }),
            1,
        );
        store.save_history(&history).unwrap();
        let loaded = match store.load_history("history", HistoryPolicies::default()).unwrap() {
            LoadHistoryOutcome::Loaded(h) => h,
            other => panic!("expected Loaded, got {other:?}"),
        };
        let item = loaded.get(0).unwrap();
        match &item.kind {
            crate::item::ItemKind::Text { rich, .. } => {
                assert_eq!(rich.as_ref().unwrap().html.as_deref(), Some("<b>rich</b>"));
            }
            other => panic!("expected text item, got {other:?}"),
        }
    }
}