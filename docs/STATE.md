# STATE — cosmic-paste

Last updated: 2026-06-13 (MVP: clipboard capture + panel applet working)

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

- **PR 7a** — ashpd GlobalShortcuts spike (`show-history`), `PortalShortcutsAvailable` property, `just test-portal`, 41 tests
- **PR 10** — `cosmic-paste` CLI: `history`, `select`, `add`, `prev`/`next`, `track`, `show-history`, `version`, `empty`, `daemon-reexec`; exit codes 0/1/2/3
- **PR 8 (MVP)** — panel applet: icon + tooltip (`N/count: preview`), click popup with history list, DBus signal subscription, `OnAppletStateChanged`, desktop + dbus service files; `just install-user` installs applet
- **ShowHistory (partial)** — daemon emits signal when applet present; portal spike + CLI `show-history` wired
- **MVP milestone** — real clipboard capture (Ctrl+C), history ingest, panel tooltip counter, tray popup; monitor fix (non-blocking offer read, offer lifecycle, systemd `ImportEnvironment`)

## Shortcuts (today)

**Daemon global shortcuts (PR 7a only):** only `show-history` is registered via the XDG GlobalShortcuts portal. On many COSMIC builds the portal is missing (`PortalShortcutsAvailable=false`); use COSMIC Settings custom shortcuts or CLI instead.

**Default accelerators** (GTK-style, in `settings.shortcuts` — used when PR 7 lands):

| Setting key | Default | Action |
|-------------|---------|--------|
| `show_history` | `<Ctrl><Alt>H` | Open history popup |
| `launch_ui` | `<Ctrl><Alt>G` | Full history window (not built yet) |
| `select_previous` | `<Ctrl><Alt>Up` | Older item (`SelectAtOffset +1`) |
| `select_next` | `<Ctrl><Alt>Down` | Newer item (`SelectAtOffset -1`) |
| `pop` | `<Ctrl><Alt>V` | Pop top item |
| `quick_select_0`…`_9` | *(empty)* | Opt-in quick pick |

**Works now via CLI** (bind in **Settings → Keyboard → Custom shortcuts** with `Spawn("…")`):

| Command | Effect |
|---------|--------|
| `cosmic-paste prev` | Newer history item + paste to clipboard |
| `cosmic-paste next` | Older history item + paste to clipboard |
| `cosmic-paste show-history` | Open panel popup (applet must be in panel) |

Example F9–F12 bindings: see `data/examples/cosmic-custom-shortcuts.ron` and `just show-cosmic-shortcuts`.

## Next

1. **PR 7** — full shortcut table (prev/next/show-history/launch-ui) + ShowHistory fallback chain
2. **PR 7b** — minimal `cosmic-paste-ui --popup` for ShowHistory fallback when applet absent
3. **PR 9** — applet pagination, keyboard nav, Ctrl+overlay
4. **PR 4 follow-up (optional)** — ext-data-control fallback for compositors without wlr-data-control

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