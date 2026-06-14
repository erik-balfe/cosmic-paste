# cosmic-paste on-disk layout

Default root: `$XDG_DATA_HOME/cosmic-paste` (usually `~/.local/share/cosmic-paste`).

```
cosmic-paste/
├── histories/
│   ├── history.ron           # metadata + item records (RON body after binary header)
│   ├── history.ron.bak       # previous revision before last atomic save
│   ├── history.ron.corrupt   # renamed primary after unrecoverable parse failure
│   ├── history.blobs/        # image/binary payloads keyed by SHA-256 hex (future)
│   │   └── <checksum-hex>
│   └── work.ron              # additional named histories (future)
├── backups/
│   └── history-YYYY-MM-DD.ron  # manual backups (planned)
└── state.json                # session state: active_index, current_history
```

## `history.ron` format

| Offset | Content |
|--------|---------|
| 0 | 20-byte magic `COSMIC_PASTE_HISTORY\0` |
| 20 | `u32` LE format version (currently `1`) |
| 24+ | RON document (`HistoryFile`) |

Writes are atomic: `*.tmp.<pid>` in the same directory, `fsync`, then `rename(2)`.

## Recovery

1. Parse primary `*.ron`
2. On failure, parse `*.ron.bak`
3. If both fail, rename primary to `*.ron.corrupt` and start with an empty in-memory history

## Install methods

| Method | Script |
|--------|--------|
| One-line (GitHub release) | `curl -fsSL …/install-remote.sh \| bash` |
| Git checkout (build) | `./scripts/install.sh` |
| Release tarball | `./scripts/install-release.sh` inside extracted bundle |

## User install paths (`install.sh` / `install-remote.sh`)

| Artifact | Path |
|----------|------|
| Binaries | `~/.local/bin/` (`cosmic-paste-daemon`, `cosmic-paste`, `cosmic-paste-applet`, `cosmic-paste-show-history`) |
| systemd unit | `~/.config/systemd/user/com.system76.CosmicPaste.service` |
| D-Bus activation | `~/.local/share/dbus-1/services/org.system76.CosmicPaste.service` |
| Applet desktop | `~/.local/share/applications/com.system76.CosmicPaste.Applet.desktop` |
| Applet icon | `~/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicPaste.Applet-symbolic.svg` |

DBus bus name: `org.system76.CosmicPaste`. Activation uses `Type=dbus` — systemd starts the daemon when the name is first requested.