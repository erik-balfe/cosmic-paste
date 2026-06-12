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

- **ADR-001** — `docs/adr/001-clipboard-monitor.md`: wlr-data-control + dedicated thread (accepted)
- **PR 4** — `monitor/data_control.rs` (wlr-data-control), ingest loop, `SelfCopyGuard`, 35 tests
- **PR 5** — systemd `Type=dbus` user unit, D-Bus activation file, journald logging, SIGTERM flush, `Reexecute`, `just install-user`
- **PR 4 follow-up** — `Select` / `SelectAtOffset` clipboard write-back (wlr-data-control), `Update` + `ActiveIndexChanged` on clipboard ingest; review cleanup (write-first + rollback, guard window, stale sources)
- **PR 6** — `settings.rs` + cosmic_config hot-reload, `track_applet_state`, history policy mapping, 40 tests (35 core + 5 daemon)

## Next

1. **PR 7a** — ashpd GlobalShortcuts proof-of-life
2. **PR 4 follow-up (optional)** — ext-data-control fallback for compositors without wlr-data-control

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