# cosmic-paste

v0.1.0 — production-grade clipboard manager for COSMIC (libcosmic, daemon, wlr-data-control).

## Mission

Build a stable, maintainable clipboard manager that meets COSMIC APIs, design language, and quality bar — with a long-term path to become the official default clipboard manager and upstream into System76’s application set.

## Success criteria

- **COSMIC compliance:** libcosmic applet, cosmic_config settings, portal shortcuts, systemd/DBus activation patterns aligned with other System76 apps.
- **Reliability:** daemon-owned history, atomic persistence, SelfCopyGuard through paste, no silent data loss on corrupt files.
- **UX:** panel applet with scrollable history popup, keyboard navigation with clear status/toasts, one-command install.
- **Upstream readiness:** clean public repo, CI green, documented state, no personal/internal leakage, packaging stubs correct.

## Now (v0.1.0)

- Text history, persistence, panel applet, scrollable popup (100 items)
- Shortcuts: Ctrl+F9 menu, Ctrl+F11 prev, Ctrl+F12 next
- Navigation toasts: fixed 2 lines, 80-char middle-truncated preview
- Show-history: file + unix socket + DBus + DbusActivation (`cosmic-paste-show-history`)
- Install: `./scripts/install.sh`; add panel applet once in Settings

## Done

- SelfCopyGuard through clipboard write (5250 ms)
- Popup scroll id stable; skip label rebuild when history unchanged
- Review fixes: guard timeout, single show-history threads, subscription clone, `max_displayed_history_size`, `~/.local/bin` in units, toast on popup select
- CI: Rust 1.93, clippy, tests (portal smoke skips when session unavailable)

## Planned

- Popup hover/accent styling, scroll-to-active on open
- Search, private mode, pins, delete, keyboard nav in popup
- Images, rich clipboard, `cosmic-paste-ui` window
- cosmic-settings integration, named histories, packaging for distro repos
- Upstream proposal to System76 after Phase 2 polish

## Known limitations

- Portal shortcuts missing on many COSMIC builds — custom Spawn required (`data/examples/cosmic-custom-shortcuts.ron`)
- Text only; no search/private mode UI yet
- `cosmic-paste-ui` crate is a stub; full history window not shipped

## Gaps before upstream

- Visual polish to match COSMIC panel/popup patterns
- Broader manual QA on COSMIC sessions (install, shortcuts, persistence across reboot)
- Distro packaging beyond `scripts/install.sh` and Homebrew formula stub