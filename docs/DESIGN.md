# cosmic-paste — architecture

> **Third-party software** for **Linux + COSMIC desktop + Wayland** only. Not official System76/COSMIC software.
>
> **Current behavior:** [`STATE.md`](STATE.md) and source code. This document describes how v0.1.0 is built; planned work is listed at the end.

| Field | Value |
|-------|-------|
| **App ID** | `com.system76.CosmicPaste` (cosmic_config) |
| **DBus** | `org.system76.CosmicPaste` / `org.system76.CosmicPaste2` |
| **Repository** | https://github.com/erik-balfe/cosmic-paste |

---

## Overview

GPaste-inspired clipboard manager for COSMIC: a **daemon** owns clipboard monitoring, history, persistence, and global shortcuts; a **panel applet** and **CLI** talk to it over DBus.

```
cosmic-paste-core/     history, settings, persistence, DBus types
cosmic-paste-daemon/   wlr-data-control monitor, DBus service, portal shortcuts
cosmic-paste-applet/   libcosmic panel icon + hover popup
cosmic-paste-cli/      cosmic-paste command
cosmic-paste-ui/       stub (full history window — not shipped)
```

---

## Platform

| Supported | Not supported |
|-----------|---------------|
| Linux, Wayland, COSMIC session | macOS, Windows, X11, GNOME/KDE generic |
| `wlr-data-control` clipboard monitor | smithay-clipboard (not validated on COSMIC) |

Install: `install-remote.sh` (GitHub release bundle) or `./scripts/install.sh` (source) → `~/.local/bin`, user systemd unit, DBus activation file; daemon enabled automatically.

---

## Components

### Daemon (`cosmic-paste-daemon`)

- systemd user service `com.system76.CosmicPaste.service`, `Type=dbus`, bus name `org.system76.CosmicPaste`
- Monitors clipboard on a dedicated Wayland thread (`zwlr_data_control_manager_v1`), debounce 75 ms
- Ingests `text/plain` into in-memory history, atomic RON persistence under `$XDG_DATA_HOME/cosmic-paste`
- Exposes `org.system76.CosmicPaste2` on `/org/system76/CosmicPaste`
- Registers up to three **portal** global shortcuts (see below)
- `SelfCopyGuard` suppresses re-ingest during paste (window ≈ 5250 ms = write timeout + 250 ms)

### Panel applet (`cosmic-paste-applet`)

- Must run inside **cosmic-panel** (`WAYLAND_SOCKET` set); add once in Settings → Desktop → Panel → Applets
- Tooltip: two lines — `N/count|` then truncated preview
- Hover popup: scrollable list (up to `max_displayed_history_size` entries), click selects via `SelectAtIndex`
- Background DBus thread: signals, show-history triggers, `OnAppletStateChanged(true)` while connected

### CLI (`cosmic-paste`)

`history`, `prev`, `next`, `show-history`, `add`, `select`, `version`, … — all via session DBus.

---

## History and active index

- Index **0** = newest item. Higher index = older.
- **`SelectAtOffset(-1)`** → newer (toward 0). **`SelectAtOffset(+1)`** → older.
- Default **`navigation_wrap`** = `false` (clamp at ends).
- **`prev` CLI / `select_previous` setting** → offset −1 (newer). **`next` / `select_next`** → offset +1 (older). Names follow GPaste-style IDs, not “back in time” English.

| Setting (default) | Value |
|-------------------|-------|
| `max_history_size` | 100 |
| `max_displayed_history_size` | 100 (DBus `GetHistory` and popup) |
| `max_text_item_size` | 1_048_575 |
| `element_size` | 72 (display truncation) |

---

## Keyboard shortcuts

Two mechanisms — **only one is active** on a given machine.

### 1. GlobalShortcuts portal (daemon)

The freedesktop **XDG Desktop Portal** API `org.freedesktop.portal.GlobalShortcuts` lets the daemon register global keys without COSMIC Settings entries. Implemented in `cosmic-paste-daemon/src/shortcuts/portal_spike.rs` via `ashpd`.

**Default bindings** (overridable in cosmic_config `shortcuts`):

| Accelerator | Portal ID | Action |
|-------------|-----------|--------|
| `<Ctrl>F9` | `select-previous` | Newer item (`SelectAtOffset(-1)`) |
| `<Ctrl>F10` | `select-next` | Older item (`SelectAtOffset(+1)`) |
| `<Ctrl>F11` | `show-history` | Open history popup |

DBus read-only property **`PortalShortcutsAvailable`** reflects whether bind succeeded.

**On many COSMIC installs the portal bind fails or the interface is absent.** Example from this dev machine: daemon running, `PortalShortcutsAvailable = false`. That is expected — not a bug in cosmic-paste alone.

**Limitation (v0.1.0):** shortcut accelerators are read at **daemon start**. Changing `shortcuts` in cosmic_config does not re-bind portal until `systemctl --user restart com.system76.CosmicPaste.service`.

Other shortcut fields in settings (`launch_ui`, `pop`, `mark_password`, quick-select, …) exist for future use; **only the three above are wired** to the portal.

### 2. COSMIC custom shortcuts (fallback)

When the portal is unavailable, use **Settings → Keyboard → Custom shortcuts** (Spawn actions). Copy from [`data/examples/cosmic-custom-shortcuts.ron`](../data/examples/cosmic-custom-shortcuts.ron) and replace `@bindir@` with your install path (e.g. `~/.local/bin`):

| Key | Spawn |
|-----|-------|
| Ctrl+F9 | `cosmic-paste prev` |
| Ctrl+F10 | `cosmic-paste next` |
| Ctrl+F11 | `cosmic-paste-show-history` |

Use the **show-history helper script**, not `cosmic-paste show-history` alone — it hits DBus + unix socket + DbusActivation synchronously (better for shortcut latency).

**If shortcuts work after merging custom RON, you do not need the portal** — same bindings, different delivery path.

---

## Show-history paths

When popup should open:

1. DBus signal `ShowHistory` → applet subscription
2. Unix socket + file trigger under `$XDG_RUNTIME_DIR/cosmic-paste/` (helper script)
3. `cosmic-paste-show-history` → gdbus `ShowHistory` + applet `DbusActivation`
4. Applet must be in the panel; otherwise user sees CLI hint to add it

`cosmic-paste-ui --popup` is **not implemented** (DESIGN PR 7b stub only).

---

## Clipboard write-back

Paste selection uses **`wl-copy`** with `SelfCopyGuard` armed first (fast, reliable in COSMIC apps). A Wayland data-control write queue exists for other paths; navigation/paste uses wl-copy only after race issues with dual writes.

---

## DBus API (`org.system76.CosmicPaste2`)

**Implemented (v0.1.0):** `Add`, `GetHistory`, `GetActiveIndex`, `SetActiveIndex`, `Select`, `SelectAtIndex`, `SelectAtOffset`, `ShowHistory`, `Track`/`Active`, `OnAppletStateChanged`, `GetElementAtIndex`, `EmptyHistory`, `Reexecute`, `Version`, properties `ActiveIndex`, `AppletPresent`, `PortalShortcutsAvailable`, signals `Update`, `ActiveIndexChanged`, `ShowHistory`.

**Returns `NotSupported`:** passwords, images, search, merge, named histories, `About`, upload, etc. — see `service.rs`.

GPaste2-shaped API is the long-term direction; many methods are stubs.

---

## Persistence

See [`data/state-paths.md`](../data/state-paths.md). Magic header `COSMIC_PASTE_HISTORY`, atomic rename writes, `.bak` recovery.

---

## Clipboard monitor

ADR: [`docs/adr/001-clipboard-monitor.md`](adr/001-clipboard-monitor.md) — **wlr-data-control** on a blocking thread, `mpsc` to tokio ingest loop.

---

## Security notes

- Text only in v0.1.0; no password obfuscation yet
- History files under user data dir; respect file permissions on shared systems
- Self-copy guard prevents navigation from corrupting `active_index` via stale clipboard events

---

## Planned (not in v0.1.0)

See [`STATE.md`](STATE.md): popup polish, search, private mode, images/rich text, `cosmic-paste-ui`, cosmic-settings integration, portal shortcut hot-reload, extra GPaste shortcut bindings, distro packaging.

---

## References

- [GPaste](https://github.com/Keruspe/GPaste) — feature reference
- [cosmic-applet-template](https://github.com/pop-os/cosmic-applet-template)
- [cosmic-utils/clipboard-manager](https://github.com/pop-os/cosmic-utils/tree/master/clipboard-manager) — wlr-data-control prior art
- [ashpd GlobalShortcuts](https://docs.rs/ashpd/latest/ashpd/desktop/global_shortcuts/index.html)