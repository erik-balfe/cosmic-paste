# cosmic-paste

v0.1.0 — third-party clipboard manager for the COSMIC desktop (libcosmic, daemon, wlr-data-control).

## Project status

**Independent software.** Not affiliated with System76 or the official COSMIC application set.

## Mission

Build a stable, maintainable clipboard manager that follows COSMIC APIs, design language, and coding conventions — quality software for the desktop, with a long-term goal to propose it upstream if it earns that place.

## Success criteria

- **COSMIC integration:** libcosmic applet, cosmic_config settings, portal shortcuts, systemd/DBus activation patterns consistent with other COSMIC apps.
- **Reliability:** daemon-owned history, atomic persistence, SelfCopyGuard through paste, no silent data loss on corrupt files.
- **UX:** panel applet with scrollable history popup, keyboard navigation with clear status/toasts, one-command install.
- **Documentation:** known limitations and third-party status documented; packaging stubs kept current.

## Now (v0.1.0)

- Text history, persistence, panel applet, scrollable popup (100 items)
- Shortcuts: Ctrl+F9 menu, Ctrl+F11 prev, Ctrl+F12 next (portal; see limitations)
- Navigation toasts: fixed 2 lines, 80-char middle-truncated preview
- Show-history: file + unix socket + DBus + DbusActivation (`cosmic-paste-show-history`)
- Install: `./scripts/install.sh`; add panel applet once in Settings

## Done

- SelfCopyGuard through clipboard write (5250 ms)
- Popup scroll id stable; skip label rebuild when history unchanged
- Guard timeout, single show-history threads, subscription clone, `max_displayed_history_size`, `~/.local/bin` in units, toast on popup select
- CI: Rust 1.93, clippy, tests (portal smoke skips when session unavailable)

## Planned

- Popup hover/accent styling, scroll-to-active on open
- Search, private mode, pins, delete, keyboard nav in popup
- Images, rich clipboard, `cosmic-paste-ui` window
- cosmic-settings integration, named histories, distro packaging
- Broader testing on COSMIC sessions (install, shortcuts, persistence across reboot)
- Upstream proposal to COSMIC maintainers after Phase 2 polish

## Known limitations

- Portal shortcuts missing on many COSMIC builds — custom Spawn required (`data/examples/cosmic-custom-shortcuts.ron`); see README
- Text only; no search/private mode UI yet
- `cosmic-paste-ui` crate is a stub; full history window not shipped