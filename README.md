# cosmic-paste

Clipboard manager for the [COSMIC](https://github.com/pop-os/cosmic-epoch) desktop.

**Not official.** This is independent, third-party software. It is **not** made, endorsed, or maintained by System76 or the COSMIC project. The name refers to the desktop it targets — I use COSMIC daily and built this to work well with it.

**Status:** [`docs/STATE.md`](docs/STATE.md)

Default shortcuts (daemon portal, when available):

| Key | Action |
|-----|--------|
| Ctrl+F9 | Forward — newer clipboard item |
| Ctrl+F10 | Back — older clipboard item |
| Ctrl+F11 | Open history popup |

On many COSMIC builds the portal is unavailable — merge [`data/examples/cosmic-custom-shortcuts.ron`](data/examples/cosmic-custom-shortcuts.ron) into COSMIC Settings → Keyboard → Custom shortcuts (same bindings; use `cosmic-paste-show-history` for F11).

## Install

```bash
./scripts/install.sh
```

Then: Settings → Panel → Applets → COSMIC Paste.

## Build

```bash
just test
just check
```

## CLI

`cosmic-paste history | prev | next | show-history | add 'text' | version`

BSD-2-Clause