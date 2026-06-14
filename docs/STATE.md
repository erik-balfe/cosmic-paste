# cosmic-paste

v0.1.0 — clipboard manager for COSMIC (libcosmic, daemon, wlr-data-control).

## Now

- Text history, persistence, panel applet, scrollable popup (100 items)
- Shortcuts: Ctrl+F9 menu, Ctrl+F11 prev, Ctrl+F12 next
- Navigation toasts: fixed 2 lines, 80-char middle-truncated preview
- Show-history: file + unix socket + DBus + DbusActivation (`cosmic-paste-show-history`)
- Install: `./scripts/install.sh`; add panel applet once in Settings

## Done

- SelfCopyGuard through clipboard write (5250 ms)
- Popup scroll id stable; skip label rebuild when history unchanged
- Review fixes: guard timeout, single show-history threads, subscription clone, `max_displayed_history_size`, `~/.local/bin` in units, toast on popup select

## Planned

- Popup hover/accent styling, scroll-to-active on open
- Search, private mode, pins, delete, keyboard nav in popup
- Images, rich clipboard, `cosmic-paste-ui` window

## Gaps

- Portal shortcuts missing on many COSMIC builds — custom Spawn required (`data/examples/cosmic-custom-shortcuts.ron`)
- Text only; no search/private mode UI yet