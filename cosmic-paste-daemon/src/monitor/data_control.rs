//! wlr-data-control clipboard monitor (ADR-001).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cosmic_paste_core::item::text_checksum;
use tokio::sync::mpsc;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{
    event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
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

struct State {
    seats: Vec<(WlSeat, SeatData)>,
    offers: HashMap<ZwlrDataControlOfferV1, HashSet<String>>,
    pending: Option<PendingSelection>,
    config: MonitorConfig,
    guard: Arc<Mutex<SelfCopyGuard>>,
    tx: mpsc::Sender<ClipboardEvent>,
}

pub fn run(
    tx: mpsc::Sender<ClipboardEvent>,
    config: MonitorConfig,
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
        seats,
        offers: HashMap::new(),
        pending: None,
        config,
        guard,
        tx,
    };

    tracing::info!("wlr-data-control clipboard monitor connected");

    loop {
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

        if pending.changed_at.elapsed() < self.config.debounce {
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
            .expect("self-copy guard lock")
            .should_ignore(fingerprint)
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
                if state.config.watch_primary && let Some(offer) = id {
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