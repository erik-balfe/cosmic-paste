# cosmic-paste on-disk layout

Default root: `$XDG_DATA_HOME/cosmic-paste` (usually `~/.local/share/cosmic-paste`).

```
cosmic-paste/
├── histories/
│   ├── history.ron           # metadata + item records (RON body after binary header)
│   ├── history.ron.bak       # previous revision before last atomic save
│   ├── history.ron.corrupt   # renamed primary after unrecoverable parse failure
│   ├── history.blobs/        # image/binary payloads keyed by SHA-256 hex
│   │   └── <checksum-hex>
│   └── work.ron              # additional named histories
├── backups/
│   └── history-2026-06-13.ron  # manual backups (PR 14a)
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