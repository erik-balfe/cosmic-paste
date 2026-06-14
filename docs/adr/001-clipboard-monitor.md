# ADR-001: Clipboard monitor backend

**Status:** Accepted (spike complete — 2026-06-13)  
**Context:** Daemon clipboard monitoring per `docs/DESIGN.md`.

## Decision

Use **`zwlr_data_control_manager_v1` (wlr-data-control)** on a **dedicated OS thread**, forwarding decoded clipboard payloads to the tokio runtime via **`tokio::sync::mpsc`**.

Rejected for v1:

| Option | Verdict | Reason |
|--------|---------|--------|
| **smithay-clipboard** | Reject | No production use in COSMIC workspace; unvalidated on cosmic-comp |
| **arboard** | Reject | Text/image focused; weak primary-selection + MIME story on Wayland |

## Evidence (COSMIC)

1. **cosmic-comp** exposes both `wlr_data_control` and `ext_data_control` globals (`cosmic-comp/src/state.rs`).
2. **cosmic-utils/clipboard-manager** (`src/clipboard_watcher.rs`) binds `ZwlrDataControlManagerV1`, creates per-seat `ZwlrDataControlDeviceV1`, watches `Selection` / `PrimarySelection` events, reads offers via pipe + `receive()`.
3. clipboard-manager runs the watcher on **`tokio::task::spawn_blocking`** and bridges to iced via `mpsc` — same isolation principle cosmic-paste needs for a headless daemon.

## Threading model

```text
┌──────────────────────────────────────────────┐
│ Dedicated Wayland thread (blocking)          │
│  • Connection::connect_to_env()              │
│  • EventQueue dispatch loop                  │
│  • debounce (75 ms default)                  │
│  • MIME read via offer.receive() + pipe      │
│  • SelfCopyGuard fingerprint check           │
└──────────────────┬───────────────────────────┘
                   │ tokio::sync::mpsc
                   ▼
┌──────────────────────────────────────────────┐
│ Tokio runtime (daemon main)                  │
│  • ingest → HistorySession (Mutex)           │
│  • persist + DBus signals                    │
│  • Select write-back (future PR)             │
└──────────────────────────────────────────────┘
```

**Rationale:** wlr-data-control is callback/event-queue driven. Running it on the tokio runtime would block worker threads or require fragile `spawn_blocking` per dispatch. A long-lived dedicated thread matches clipboard-manager and keeps DBus/ashpd responsive.

## MIME precedence (ingest)

When multiple MIME types arrive on one offer:

1. `text/uri-list`
2. `image/*` (png, bmp, …)
3. `text/html` (+ store `text/plain` fallback when rich-text enabled)
4. `text/plain`
5. `application/x-color` / `x-kde-color`

PR 4 implements **text/plain only**; precedence resolver stub logs other types.

## Self-copy guard

Before daemon writes clipboard on `Select` / `SelectAtOffset` (future):

- Record fingerprint (SHA-256 of canonical body) + 500 ms ignore window.
- Monitor drops matching `Selection` notifies inside the window.

## Multi-seat (v1)

- Bind **first seat** from registry (`Seat::Unspecified` — same as clipboard-manager).
- Log when `seats.len() > 1`; document single-seat-first limitation (US-151).

## Latency target

- Notify → `HistorySession::ingest_text` < **100 ms p99** for 4 KB `text/plain` on COSMIC.
- Measured in PR 4 integration test (`monitor` + in-memory history); hardware validation on user session.

## Implementation path

| Component | Location |
|-----------|----------|
| Event types + guard | `cosmic-paste-daemon/src/monitor/mod.rs` |
| wlr-data-control thread | `cosmic-paste-daemon/src/monitor/data_control.rs` (PR 4) |
| Ingest bridge | `cosmic-paste-daemon/src/ingest.rs` (PR 4) |

**Dependencies (PR 4):** `wayland-client`, `wayland-protocols-wlr` — not `libcosmic` (headless daemon).

## References

- [wlr-data-control protocol](https://wayland.app/protocols/wlr-data-control-unstable-v1)
- [cosmic-utils/clipboard-manager `clipboard_watcher.rs`](https://github.com/cosmic-utils/clipboard-manager/blob/master/src/clipboard_watcher.rs)
- `docs/DESIGN.md` §daemon — clipboard monitoring