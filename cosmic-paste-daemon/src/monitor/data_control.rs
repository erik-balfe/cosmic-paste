//! wlr-data-control clipboard monitor (ADR-001).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::event::{poll, PollFd, PollFlags};
use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};

use cosmic_paste_core::dbus::clipboard::WRITE_TIMEOUT;

/// How long to wait for compositor `Send` before acknowledging the DBus caller.
const TRANSFER_WAIT: Duration = Duration::from_millis(400);
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

use cosmic_paste_core::dbus::SharedSelfCopyGuard;

use super::{ClipboardEvent, MonitorConfig, SelectionSource};

const CLIPBOARD_WRITE_MIMES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

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
    has_text_mime: bool,
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
    guard: SharedSelfCopyGuard,
    tx: async_mpsc::Sender<ClipboardEvent>,
    write_rx: mpsc::Receiver<ClipboardWriteRequest>,
}

pub fn run(
    tx: async_mpsc::Sender<ClipboardEvent>,
    write_rx: mpsc::Receiver<ClipboardWriteRequest>,
    config: Arc<Mutex<MonitorConfig>>,
    guard: SharedSelfCopyGuard,
    shutdown: Arc<AtomicBool>,
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

    // Drain the compositor's initial selection broadcast for new devices.
    queue.roundtrip(&mut state)?;
    while queue.dispatch_pending(&mut state)? > 0 {}

    tracing::info!("wlr-data-control clipboard monitor connected");

    while !shutdown.load(Ordering::Relaxed) {
        state.expire_stale_sources();
        state.process_write_requests(&qh, &mut queue)?;

        while queue.dispatch_pending(&mut state)? > 0 {
            state.process_write_requests(&qh, &mut queue)?;
            if let Err(err) = state.flush_pending(&mut queue) {
                tracing::info!("stopping clipboard monitor: {err}");
                return Ok(());
            }
        }
        if let Err(err) = state.flush_pending(&mut queue) {
            tracing::info!("stopping clipboard monitor: {err}");
            return Ok(());
        }

        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        if let Err(err) = queue.flush() {
            return Err(MonitorError::Dispatch(wayland_client::DispatchError::Backend(
                err,
            )));
        }
        let Some(read_guard) = queue.prepare_read() else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };

        let fd = read_guard.connection_fd();
        let mut poll_fds = [PollFd::from_borrowed_fd(fd, PollFlags::IN)];
        match poll(&mut poll_fds, 200) {
            Ok(0) => {}
            Ok(_) if poll_fds[0].revents().contains(PollFlags::IN) => {
                if let Err(err) = read_guard.read() {
                    return Err(MonitorError::Dispatch(wayland_client::DispatchError::Backend(
                        err,
                    )));
                }
                while queue.dispatch_pending(&mut state)? > 0 {
                    state.process_write_requests(&qh, &mut queue)?;
                    if let Err(err) = state.flush_pending(&mut queue) {
                        tracing::info!("stopping clipboard monitor: {err}");
                        return Ok(());
                    }
                }
                if let Err(err) = state.flush_pending(&mut queue) {
                    tracing::info!("stopping clipboard monitor: {err}");
                    return Ok(());
                }
            }
            Ok(_) => {}
            Err(err) => tracing::warn!("clipboard monitor poll error: {err}"),
        }
    }

    tracing::info!("clipboard monitor stopped");
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
        if self.clipboard_offer.as_ref() == offer.as_ref() {
            return;
        }
        if let Some(old) = self.clipboard_offer.take() {
            old.destroy();
        }
        self.clipboard_offer = offer;
    }

    fn set_primary_offer(&mut self, offer: Option<ZwlrDataControlOfferV1>) {
        if self.primary_offer.as_ref() == offer.as_ref() {
            return;
        }
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
            tracing::debug!("expiring stale clipboard write source");
            self.drop_source(&source);
        }
    }

    fn process_write_requests(
        &mut self,
        qh: &QueueHandle<Self>,
        queue: &mut EventQueue<State>,
    ) -> Result<(), MonitorError> {
        while let Ok(request) = self.write_rx.try_recv() {
            if let Ok(mut guard) = self.guard.lock() {
                guard.arm(request.fingerprint);
            }

            let source = self.manager.create_data_source(qh, ());
            for mime in CLIPBOARD_WRITE_MIMES {
                source.offer((*mime).to_string());
            }
            self.sources.insert(
                source.clone(),
                SourcePayload {
                    data: request.text.into_bytes(),
                    posted_at: Instant::now(),
                    reply: Some(request.reply),
                },
            );

            let device = self
                .seats
                .first()
                .and_then(|(_, seat_data)| seat_data.device.as_ref().cloned());
            let Some(device) = device else {
                self.fail_source(
                    &source,
                    if self.seats.is_empty() {
                        "no Wayland seat available"
                    } else {
                        "data-control device unavailable"
                    },
                );
                continue;
            };

            queue.roundtrip(self).map_err(MonitorError::Dispatch)?;
            device.set_selection(Some(&source));
            queue.flush().map_err(|err| {
                MonitorError::Dispatch(wayland_client::DispatchError::Backend(err))
            })?;

            tracing::debug!(bytes = self.sources[&source].data.len(), "posted clipboard write");
            self.wait_for_source_transfer(queue, &source, TRANSFER_WAIT)?;

            // cosmic-comp may apply the selection before Send; acknowledge once posted.
            if let Some(payload) = self.sources.get_mut(&source)
                && let Some(reply) = payload.reply.take()
            {
                let _ = reply.send(Ok(()));
            }
        }
        Ok(())
    }

    fn drop_source(&mut self, source: &ZwlrDataControlSourceV1) {
        self.sources.remove(source);
        source.destroy();
    }

    fn wait_for_source_transfer(
        &mut self,
        queue: &mut EventQueue<State>,
        source: &ZwlrDataControlSourceV1,
        timeout: Duration,
    ) -> Result<(), MonitorError> {
        let deadline = Instant::now() + timeout;
        while self.sources.contains_key(source) && Instant::now() < deadline {
            while queue.dispatch_pending(self)? > 0 {}

            queue.flush().map_err(|err| {
                MonitorError::Dispatch(wayland_client::DispatchError::Backend(err))
            })?;

            if !self.sources.contains_key(source) {
                return Ok(());
            }

            let Some(read_guard) = queue.prepare_read() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };

            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout_ms = remaining.as_millis().min(50) as i32;
            if timeout_ms == 0 {
                continue;
            }

            let fd = read_guard.connection_fd();
            let mut poll_fds = [PollFd::from_borrowed_fd(fd, PollFlags::IN)];
            match poll(&mut poll_fds, timeout_ms) {
                Ok(0) => {}
                Ok(_) if poll_fds[0].revents().contains(PollFlags::IN) => {
                    if let Err(err) = read_guard.read() {
                        return Err(MonitorError::Dispatch(wayland_client::DispatchError::Backend(
                            err,
                        )));
                    }
                }
                Ok(_) => {}
                Err(err) => tracing::warn!("clipboard write poll error: {err}"),
            }
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

    fn schedule_selection(&mut self, source: SelectionSource, offer: ZwlrDataControlOfferV1) {
        let has_text_mime = self
            .offers
            .get(&offer)
            .and_then(text_plain_mime)
            .is_some();
        self.pending = Some(PendingSelection {
            source,
            offer,
            changed_at: Instant::now(),
            has_text_mime,
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

        let has_text_mime = pending.has_text_mime
            || self
                .offers
                .get(&pending.offer)
                .and_then(text_plain_mime)
                .is_some();
        if !has_text_mime {
            return Ok(());
        }

        let pending = self.pending.take().expect("pending selection");
        let Some(payload) = read_text_plain(queue, self, &pending.offer)? else {
            self.pending = Some(PendingSelection {
                has_text_mime: true,
                ..pending
            });
            return Ok(());
        };

        let text = match std::str::from_utf8(&payload) {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("clipboard payload is not valid UTF-8: {err}");
                return Ok(());
            }
        };

        if text.is_empty() {
            return Ok(());
        }

        let fingerprint = text_checksum(text);
        if let Ok(mut guard) = self.guard.lock()
            && guard.should_suppress_ingest(fingerprint)
        {
            tracing::debug!("suppressing clipboard notification during pending write");
            guard.clear_if_matched(fingerprint);
            return Ok(());
        }

        let preview: String = text.chars().take(48).collect();
        tracing::info!(bytes = payload.len(), %preview, "clipboard captured");

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

fn is_text_transfer_mime(mime_type: &str) -> bool {
    mime_type == "text/plain"
        || mime_type.starts_with("text/plain")
        || matches!(mime_type, "UTF8_STRING" | "STRING" | "TEXT")
}

fn text_plain_mime(types: &HashSet<String>) -> Option<String> {
    for candidate in [
        "text/plain;charset=utf-8",
        "text/plain",
        "TEXT",
        "STRING",
        "UTF8_STRING",
    ] {
        if types.contains(candidate) {
            return Some(candidate.to_owned());
        }
    }
    types
        .iter()
        .find(|mime| mime.starts_with("text/plain"))
        .cloned()
}

fn read_text_plain(
    queue: &mut EventQueue<State>,
    state: &mut State,
    offer: &ZwlrDataControlOfferV1,
) -> Result<Option<Vec<u8>>, MonitorError> {
    let mime_type = match state.offers.get(offer).and_then(text_plain_mime) {
        Some(mime_type) => mime_type,
        None => return Ok(None),
    };

    let (mut reader, writer) = std::io::pipe().map_err(|err| {
        MonitorError::Dispatch(wayland_client::DispatchError::Backend(
            wayland_client::backend::WaylandError::Io(err),
        ))
    })?;

    let flags = fcntl_getfl(&reader).map_err(errno_to_dispatch_err)?;
    fcntl_setfl(&reader, flags | OFlags::NONBLOCK).map_err(errno_to_dispatch_err)?;

    offer.receive(mime_type, writer.as_fd());
    drop(writer);

    let deadline = Instant::now() + Duration::from_millis(500);
    let mut payload = Vec::new();
    let mut buf = [0u8; 4096];

    while Instant::now() < deadline {
        while queue.dispatch_pending(state)? > 0 {
            state.process_write_requests(&queue.handle(), queue)?;
        }

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => payload.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(io_to_dispatch_err(err)),
            }
        }

        if !payload.is_empty() {
            return Ok(Some(payload));
        }

        queue.flush().map_err(|err| {
            MonitorError::Dispatch(wayland_client::DispatchError::Backend(err))
        })?;

        let Some(read_guard) = queue.prepare_read() else {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };

        let fd = read_guard.connection_fd();
        let mut poll_fds = [PollFd::from_borrowed_fd(fd, PollFlags::IN)];
        match poll(&mut poll_fds, 50) {
            Ok(0) => {}
            Ok(_) if poll_fds[0].revents().contains(PollFlags::IN) => {
                if let Err(err) = read_guard.read() {
                    return Err(MonitorError::Dispatch(wayland_client::DispatchError::Backend(
                        err,
                    )));
                }
            }
            Ok(_) => {}
            Err(err) => tracing::warn!("clipboard read poll error: {err}"),
        }
    }

    Ok(None)
}

fn io_to_dispatch_err(err: std::io::Error) -> MonitorError {
    MonitorError::Dispatch(wayland_client::DispatchError::Backend(
        wayland_client::backend::WaylandError::Io(err),
    ))
}

fn errno_to_dispatch_err(err: rustix::io::Errno) -> MonitorError {
    io_to_dispatch_err(std::io::Error::from(err))
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
                tracing::debug!(offer_id = ?id.id(), "clipboard data offer");
                state.offers.insert(id, HashSet::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                tracing::info!(
                    offer = ?id.as_ref().map(|offer| offer.id()),
                    "clipboard selection changed"
                );
                let same_offer = state.pending.as_ref().is_some_and(|pending| {
                    id.as_ref()
                        .is_some_and(|offer| pending.offer.id() == offer.id())
                });
                if !same_offer {
                    state.pending = None;
                }
                data.set_clipboard_offer(id.clone());
                if let Some(offer) = id
                    && !same_offer
                {
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
            tracing::debug!(offer_id = ?offer.id(), %mime_type, "clipboard offer mime");
            types.insert(mime_type);
            if let Some(pending) = state.pending.as_mut()
                && pending.offer == *offer
                && text_plain_mime(types).is_some()
            {
                pending.has_text_mime = true;
            }
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
                if !is_text_transfer_mime(&mime_type) {
                    state.fail_source(
                        source,
                        &format!("unexpected clipboard mime type: {mime_type}"),
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

                tracing::debug!(%mime_type, bytes = data.len(), "clipboard write transfer");
                if let Err(err) = result {
                    state.fail_source(source, &err);
                } else {
                    state.drop_source(source);
                }
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                state.fail_source(source, "clipboard transfer cancelled");
            }
            _ => {}
        }
    }
}