# cosmic-paste

GPaste-inspired clipboard manager for the [COSMIC](https://github.com/pop-os/cosmic-epoch) desktop.

- **Design:** [`docs/DESIGN.md`](docs/DESIGN.md)
- **Status:** [`docs/STATE.md`](docs/STATE.md)

## Workspace

| Crate | Binary | Role |
|-------|--------|------|
| `cosmic-paste-core` | — | History, items, active-index state machine |
| `cosmic-paste-daemon` | `cosmic-paste-daemon` | DBus server (clipboard monitor in PR 4) |
| `cosmic-paste-applet` | `cosmic-paste-applet` | Panel indicator + popup (stub) |
| `cosmic-paste-ui` | `cosmic-paste-ui` | Full history window (stub) |
| `cosmic-paste-cli` | `cosmic-paste` | CLI client (stub) |

## Build

```bash
just          # release build
just test     # unit tests
just check    # clippy
```

## Development

PRs 1–3 are in tree: core types, persistence, and `CosmicPaste2` DBus skeleton. Next: ADR-001 clipboard monitor spike, then PR 4 text ingest.