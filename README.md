# cosmic-paste

Clipboard manager for the [COSMIC](https://github.com/pop-os/cosmic-epoch) desktop on **Linux (Wayland)**.

**Not official.** Independent third-party software — not made, endorsed, or maintained by System76 or the COSMIC project.

**Status:** [`docs/STATE.md`](docs/STATE.md) · **Architecture:** [`docs/DESIGN.md`](docs/DESIGN.md)

## Keyboard shortcuts

Default bindings:

| Key | Action |
|-----|--------|
| Ctrl+F9 | Forward — newer clipboard item |
| Ctrl+F10 | Back — older clipboard item |
| Ctrl+F11 | Open history popup |

### Portal vs custom shortcuts

The daemon can register these keys via the **XDG GlobalShortcuts portal** (freedesktop DBus API). When that works, shortcuts need no COSMIC Settings entries. Check:

```bash
busctl --user get-property org.system76.CosmicPaste /org/system76/CosmicPaste \
  org.system76.CosmicPaste2 PortalShortcutsAvailable
```

`true` = portal handles Ctrl+F9/F10/F11. **`false` is common on COSMIC** — use custom shortcuts instead:

1. Open **Settings → Keyboard → Custom shortcuts**
2. Merge [`data/examples/cosmic-custom-shortcuts.ron`](data/examples/cosmic-custom-shortcuts.ron) into `~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom`
3. Replace `@bindir@` with your binary path (e.g. `~/.local/bin`)
4. Restart is not required for custom shortcuts; restart the daemon only if you changed cosmic_config `shortcuts` for portal mode

Same key bindings either way. Custom shortcuts are the supported path when the portal is absent.

## Install

```bash
./scripts/install.sh
```

Then: **Settings → Desktop → Panel → Applets → End segment → COSMIC Paste** (if the icon is missing: `killall cosmic-panel`)

## Build

```bash
just test
just check
```

## CLI

`cosmic-paste history | prev | next | show-history | add 'text' | version`

- `prev` = newer (index toward 0), `next` = older — see [`docs/DESIGN.md`](docs/DESIGN.md)

BSD-2-Clause