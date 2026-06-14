# cosmic-paste

v0.1.0 — third-party clipboard manager for **Linux + COSMIC + Wayland** (libcosmic, daemon, wlr-data-control).

## Project status

**Independent software.** Not affiliated with System76 or the official COSMIC application set. **Linux only** — no macOS or Windows builds.

## Mission

Build a stable, maintainable clipboard manager that follows COSMIC APIs, design language, and coding conventions — with a long-term goal to propose it upstream if it earns that place.

## Success criteria

- **COSMIC integration:** libcosmic applet, cosmic_config settings, systemd/DBus activation patterns consistent with other COSMIC apps.
- **Reliability:** daemon-owned history, atomic persistence, SelfCopyGuard through paste, no silent data loss on corrupt files.
- **UX:** panel applet with scrollable history popup, keyboard navigation with clear status/toasts, one-command install.
- **Documentation:** matches shipped behavior; shortcuts and portal fallback explained.

## Now (v0.1.0)

- Text history, persistence, panel applet, scrollable popup (up to 100 items)
- Shortcuts: Ctrl+F9 newer, Ctrl+F10 older, Ctrl+F11 popup
- Navigation toasts: fixed 2 lines, 80-char middle-truncated preview
- Show-history: file + unix socket + DBus + DbusActivation (`cosmic-paste-show-history`)
- Install: `curl …/install-remote.sh | bash` or `./scripts/install.sh`; add panel applet once in Settings

## Shortcuts (how they work)

| Mechanism | When |
|-----------|------|
| **Portal** | Daemon binds via GlobalShortcuts at startup; `PortalShortcutsAvailable` DBus property |
| **COSMIC custom Spawn** | Fallback when portal is false — example in `data/examples/cosmic-custom-shortcuts.ron` |

Portal and custom shortcuts use the **same default keys**. Many COSMIC sessions only have the custom path. Saved settings in `~/.config/cosmic/com.system76.CosmicPaste/v1/` override code defaults until edited.

## Done

- SelfCopyGuard through clipboard write (5250 ms)
- Popup scroll id stable; skip label rebuild when history unchanged
- Guard timeout, show-history threads, subscription clone, `max_displayed_history_size`, `~/.local/bin` in units, toast on popup select
- CI: Rust 1.93, clippy, tests (portal smoke skips when session unavailable)
- Docs aligned with v0.1.0 behavior (`DESIGN.md` rewrite)
- Popup matches panel applet chrome (`get_popup` + `view_window`, `menu_button`, 360px autosize)

## Planned

- Search, private mode, pins, delete, keyboard nav in popup
- Images, rich clipboard, `cosmic-paste-ui` window
- cosmic-settings integration, named histories, distro packaging
- Portal shortcut hot-reload; wire remaining `shortcuts.*` settings keys
- Broader testing on COSMIC sessions (install, shortcuts, persistence across reboot)

## Known limitations

- Portal shortcuts often unavailable on COSMIC — use custom Spawn (`data/examples/cosmic-custom-shortcuts.ron`)
- Text only; no search/private mode UI yet
- `cosmic-paste-ui` crate is a stub
- Shortcut changes in cosmic_config require daemon restart (no portal re-bind yet)