# cosmic-paste

Clipboard manager for [COSMIC](https://github.com/pop-os/cosmic-epoch).

**Status:** [`docs/STATE.md`](docs/STATE.md)

Keyboard shortcuts (Ctrl+F9/F11/F12) use the XDG GlobalShortcuts portal. On some COSMIC builds the portal is unavailable — merge [`data/examples/cosmic-custom-shortcuts.ron`](data/examples/cosmic-custom-shortcuts.ron) into COSMIC Settings → Keyboard → Custom shortcuts instead.

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