//! wlr-data-control clipboard monitor (ADR-001).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cosmic_paste_core::dbus::clipboard::WRITE_TIMEOUT;
use cosmic_paste_core::dbus::ClipboardWriteRequest;
use cosmic_paste_core::item::text_checksum;
use tokio::sync::mpsc as async_mpsc;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{
    event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

use super::{ClipboardEvent, MonitorConfig, SelectionSource, SelfCopyGuard};

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("could not connect to the Wayland compositor: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("Wayland communication error: {0}")]
    Dispatch(#[from] wayland_client::DispatchError),
    #[error("wlr-data-control protocol is not supported by the compositor")]
    MissingDataControl,
    #[error("no Wayland seats detected")]
    NoSeats,
}

struct SeatData {
    name: Option<String>,
    device: Option<ZwlrDataControlDeviceV1>,
    clipboard_offer: Option<ZwlrDataControlOfferV1>,
    primary_offer: Option<ZwlrDataControlOfferV1>,
}

struct PendingSelection {
    source: SelectionSource,
    offer: ZwlrDataControlOfferV1,
    changed_at: Instant,
}

struct SourcePayload {
    data: Vec<u8>,
    posted_at: Instant,
    reply: Option<std::sync::mpsc::SyncSender<Result<(), String>>>,
}

struct State {
    manager: ZwlrDataControlManagerV1,
    seats: Vec<(WlSeat, SeatData)>,
    offers: HashMap<ZwlrDataControlOfferV1, HashSet<String>>,
    sources: HashMap<ZwlrDataControlSourceV1, SourcePayload>,
    pending: Option<PendingSelection>,
    config: Arc<Mutex<MonitorConfig>>,
    guard: Arc<Mutex<SelfCopyGuard>>,
    tx: async_mpsc::Sender<ClipboardEvent>,
    write_rx: mpsc::Receiver<ClipboardWriteRequest>,
}

pub fn run(
    tx: async_mpsc::Sender<ClipboardEvent>,
    write_rx: mpsc::Receiver<ClipboardWriteRequest>,
    config: Arc<Mutex<MonitorConfig>>,
    guard: Arc<Mutex<SelfCopyGuard>>,
) -> Result<(), MonitorError> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|err| match err {
        wayland_client::globals::GlobalError::Backend(err) => {
            MonitorError::Dispatch(wayland_client::DispatchError::Backend(err))
        }
        wayland_client::globals::GlobalError::InvalidId(_) => MonitorError::MissingDataControl,
    })?;
    let qh = queue.handle();

    let manager = globals
        .bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .map_err(|_| MonitorError::MissingDataControl)?;

    let registry = globals.registry();
    let mut seats = globals.contents().with_list(|list| {
        list.iter()
            .filter(|global| global.interface == WlSeat::interface().name && global.version >= 2)
            .map(|global| {
                let seat = registry.bind(global.name, 2, &qh, ());
                (seat, SeatData::new())
            })
            .collect::<Vec<_>>()
    });

    if seats.is_empty() {
        return Err(MonitorError::NoSeats);
    }

    if seats.len() > 1 {
        tracing::info!(
            count = seats.len(),
            "multiple Wayland seats detected; monitoring the first seat only (US-151)"
        );
    }

    for (seat, data) in &mut seats {
        let device = manager.get_data_device(seat, &qh, seat.clone());
        data.device = Some(device);
    }

    let mut state = State {
        manager: manager.clone(),
        seats,
        offers: HashMap::new(),
        sources: HashMap::new(),
        pending: None,
        config,
        guard,
        tx,
        write_rx,
    };

    tracing::info!("wlr-data-control clipboard monitor connected");

    loop {
        state.expire_stale_sources();
        state.process_write_requests(&qh)?;
        queue.blocking_dispatch(&mut state)?;
        if let Err(err) = state.flush_pending(&mut queue) {
            tracing::info!("stopping clipboard monitor: {err}");
            break;
        }
    }

    Ok(())
}

impl SeatData {
    fn new() -> Self {
        Self {
            name: None,
            device: None,
            clipboard_offer: None,
            primary_offer: None,
        }
    }

    fn set_device(&mut self, device: Option<ZwlrDataControlDeviceV1>) {
        if let Some(old) = self.device.take() {
            old.destroy();
        }
        self.device = device;
    }

    fn set_clipboard_offer(&mut self, offer: Option<ZwlrDataControlOfferV1>) {
        if let Some(old) = self.clipboard_offer.take() {
            old.destroy();
        }
        self.clipboard_offer = offer;
    }

    fn set_primary_offer(&mut self, offer: Option<ZwlrDataControlOfferV1>) {
        if let Some(old) = self.primary_offer.take() {
            old.destroy();
        }
        self.primary_offer = offer;
    }
}

impl State {
    fn expire_stale_sources(&mut self) {
        let stale: Vec<ZwlrDataControlSourceV1> = self
            .sources
            .iter()
            .filter(|(_, payload)| payload.posted_at.elapsed() > WRITE_TIMEOUT)
            .map(|(source, _)| source.clone())
            .collect();

        for source in stale {
            self.fail_source(&source, "clipboard write timed out");
        }
    }

    fn process_write_requests(&mut self, qh: &QueueHandle<Self>) -> Result<(), MonitorError> {
        while let Ok(request) = self.write_rx.try_recv() {
            if let Ok(mut guard) = self.guard.lock() {
                guard.arm(request.fingerprint);
            }

            let source = self.manager.create_data_source(qh, ());
            source.offer("text/plain".to_string());
            self.sources.insert(
                source.clone(),
                SourcePayload {
                    data: request.text.into_bytes(),
                    posted_at: Instant::now(),
                    reply: Some(request.reply),
                },
            );

            let Some((_, seat_data)) = self.seats.first() else {
                self.fail_source(&source, "no Wayland seat available");
                continue;
            };
            let Some(device) = seat_data.device.as_ref() else {
                self.fail_source(&source, "data-control device unavailable");
                continue;
            };

            device.set_selection(Some(&source));
            tracing::debug!(bytes = self.sources[&source].data.len(), "posted clipboard write");
        }
        Ok(())
    }

    fn fail_source(&mut self, source: &ZwlrDataControlSourceV1, message: &str) {
        if let Some(payload) = self.sources.remove(source)
            && let Some(reply) = payload.reply
        {
            let _ = reply.send(Err(message.to_owned()));
        }
        source.destroy();
    }

    fn complete_source(&mut self, source: &ZwlrDataControlSourceV1, result: Result<(), String>) {
        if let Some(payload) = self.sources.remove(source)
            && let Some(reply) = payload.reply
        {
            let _ = reply.send(result);
        }
        source.destroy();
    }

    fn schedule_selection(&mut self, source: SelectionSource, offer: ZwlrDataControlOfferV1) {
        self.pending = Some(PendingSelection {
            source,
            offer,
            changed_at: Instant::now(),
        });
    }

    fn flush_pending(&mut self, queue: &mut EventQueue<State>) -> Result<(), MonitorError> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };

        let debounce = self
            .config
            .lock()
            .map(|cfg| cfg.debounce)
            .unwrap_or_default();
        if pending.changed_at.elapsed() < debounce {
            return Ok(());
        }

        let pending = self.pending.take().expect("pending selection");
        let Some(payload) = read_text_plain(queue, self, &pending.offer)? else {
            return Ok(());
        };

        let text = match std::str::from_utf8(&payload) {
            Ok(text) => text,
            Err(_) => return Ok(()),
        };

        let fingerprint = text_checksum(text);
        if self
            .guard
            .lock()
            .ok()
            .is_some_and(|guard| guard.should_ignore(fingerprint))
        {
            tracing::debug!("ignoring self-copy clipboard notification");
            return Ok(());
        }

        let event = ClipboardEvent {
            source: pending.source,
            mime_type: "text/plain".into(),
            payload,
            observed_at: unix_now(),
        };

        if self.tx.blocking_send(event).is_err() {
            tracing::info!("clipboard ingest channel closed; stopping monitor");
            return Err(MonitorError::Dispatch(wayland_client::DispatchError::Backend(
                wayland_client::backend::WaylandError::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "ingest channel closed",
                )),
            )));
        }

        Ok(())
    }
}

fn read_text_plain(
    queue: &mut EventQueue<State>,
    state: &mut State,
    offer: &ZwlrDataControlOfferV1,
) -> Result<Option<Vec<u8>>, MonitorError> {
    let mime_types = match state.offers.get(offer) {
        Some(types) if types.contains("text/plain") => types,
        _ => return Ok(None),
    };

    let _ = mime_types;

    let (mut reader, writer) = std::io::pipe().map_err(|err| {
        MonitorError::Dispatch(wayland_client::DispatchError::Backend(
            wayland_client::backend::WaylandError::Io(err),
        ))
    })?;

    offer.receive("text/plain".to_string(), writer.as_fd());
    drop(writer);

    for _ in 0..8 {
        queue.blocking_dispatch(state)?;
    }

    let mut payload = Vec::new();
    reader.read_to_end(&mut payload).map_err(|err| {
        MonitorError::Dispatch(wayland_client::DispatchError::Backend(
            wayland_client::backend::WaylandError::Io(err),
        ))
    })?;

    if payload.is_empty() {
        Ok(None)
    } else {
        Ok(Some(payload))
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_seat::Event::Name { name } = event
            && let Some((_, data)) = state.seats.iter_mut().find(|(s, _)| s == seat)
        {
            data.name = Some(name);
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, WlSeat> for State {
    fn event(
        state: &mut Self,
        _device: &ZwlrDataControlDeviceV1,
        event: <ZwlrDataControlDeviceV1 as Proxy>::Event,
        seat: &WlSeat,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some((_, data)) = state.seats.iter_mut().find(|(s, _)| s == seat) else {
            return;
        };

        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                state.offers.insert(id, HashSet::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                data.set_clipboard_offer(id.clone());
                if let Some(offer) = id {
                    state.schedule_selection(SelectionSource::Clipboard, offer);
                }
            }
            zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
                data.set_primary_offer(id.clone());
                let watch_primary = state
                    .config
                    .lock()
                    .map(|cfg| cfg.watch_primary)
                    .unwrap_or(false);
                if watch_primary && let Some(offer) = id {
                    state.schedule_selection(SelectionSource::Primary, offer);
                }
            }
            zwlr_data_control_device_v1::Event::Finished => {
                data.set_device(None);
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: <ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event
            && let Some(types) = state.offers.get_mut(offer)
        {
            types.insert(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for State {
    fn event(
        state: &mut Self,
        source: &ZwlrDataControlSourceV1,
        event: <ZwlrDataControlSourceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
                if mime_type != "text/plain" {
                    state.complete_source(
                        source,
                        Err(format!("unexpected clipboard mime type: {mime_type}")),
                    );
                    return;
                }

                let Some(data) = state.sources.get(source).map(|payload| payload.data.clone()) else {
                    return;
                };

                let result = (|| {
                    let mut file = std::fs::File::from(fd);
                    file.write_all(&data)?;
                    file.flush()?;
                    Ok(())
                })()
                .map_err(|err: std::io::Error| err.to_string());

                state.complete_source(source, result);
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                state.complete_source(source, Err("clipboard transfer cancelled".into()));
            }
            _ => {}
        }
    }
}