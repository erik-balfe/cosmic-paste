# STATE — cosmic-paste

Last updated: 2026-06-13

**Design:** [`docs/DESIGN.md`](DESIGN.md)

GPaste-inspired clipboard manager for COSMIC (libcosmic native, daemon-centric).

## Done

- Product design doc (rev 4): user stories, architecture, DBus API, 19-PR plan
- Review loop complete (0 open issues)
- User decisions: clamp navigation, tooltip-only panel index, no upload/pastebin
- Project scaffold: `~/dev/projects/cosmic-paste` with jj colocate
- **PR 1** — Cargo workspace, `cosmic-paste-core` (`HistoryItem`, `History`, `ActiveIndexState`, `HistorySession`), stub binaries, `justfile`
- **PR 2** — `persistence.rs`: RON + magic header, atomic save, `.bak` recovery, blob store, `state.json`
- **PR 3** — `org.system76.CosmicPaste2` zbus service + client proxy in `cosmic-paste-core`, daemon entrypoint, `data/dbus/org.system76.CosmicPaste.xml`; 30 tests

## Next

1. **ADR-001 spike** (before PR 4) — wlr-data-control vs alternatives on COSMIC hardware
2. **PR 4** — clipboard monitor + text ingest (gated on ADR-001)

## Key decisions (locked)

| Topic | Choice |
|-------|--------|
| UI | libcosmic native (not Tauri) |
| Clipboard monitor | wlr-data-control (pending ADR-001) |
| Shortcuts | `ashpd` + XDG GlobalShortcuts portal |
| Panel index | Tooltip `N/count: preview` for v1 |
| Prev/next | Clamp at boundaries |
| Upload | Out of scope |

## Repo layout (planned)

```
cosmic-paste-core/
cosmic-paste-daemon/
cosmic-paste-applet/
cosmic-paste-ui/
cosmic-paste-cli/
data/
```