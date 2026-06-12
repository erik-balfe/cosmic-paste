# cosmic-paste

GPaste-inspired clipboard manager for the [COSMIC](https://github.com/pop-os/cosmic-epoch) desktop.

- **Design:** [`docs/DESIGN.md`](docs/DESIGN.md)
- **Status:** [`docs/STATE.md`](docs/STATE.md)

## Workspace

| Crate | Binary | Role |
|-------|--------|------|
| `cosmic-paste-core` | — | History, items, active-index state machine |
| `cosmic-paste-daemon` | `cosmic-paste-daemon` | Clipboard monitor + DBus (stub) |
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

PR 1 delivers in-memory core types only. Next: persistence (PR 2), DBus skeleton (PR 3), then ADR-001 clipboard monitor spike before PR 4.