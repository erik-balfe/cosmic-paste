use cosmic::iced::futures::channel::mpsc as iced_mpsc;
use cosmic::iced::futures::{SinkExt, StreamExt};
use cosmic::iced::{stream, Subscription};
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::show_history_trigger;
use cosmic_paste_core::{BUS_NAME, INTERFACE_NAME, OBJECT_PATH};
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::mpsc::{
    self, Receiver, RecvTimeoutError, SyncSender, TryRecvError as StdTryRecvError,
};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use zbus::message::Type;
use zbus::zvariant::Value;
use zbus::{interface, MessageStream};

pub const APPLET_BUS_NAME: &str = "com.system76.CosmicPaste.Applet";
pub const APPLET_OBJECT_PATH: &str = "/com/system76/CosmicPaste/Applet";

#[derive(Debug, Clone)]
pub enum DbusEvent {
    Refreshed {
        history: Vec<(String, String)>,
        active_index: u32,
        tracking: bool,
    },
    ActiveIndexChanged {
        active_index: u32,
    },
    ShowHistory,
    Disconnected,
}

type EventReceiver = Arc<Mutex<Receiver<DbusEvent>>>;
static EVENT_RX: OnceLock<Mutex<Option<EventReceiver>>> = OnceLock::new();

pub async fn fetch_state() -> Result<DbusEvent, ()> {
    let conn = zbus::Connection::session().await.map_err(|_| ())?;
    let proxy = CosmicPasteProxy::builder(&conn)
        .destination(BUS_NAME)
        .map_err(|_| ())?
        .path(OBJECT_PATH)
        .map_err(|_| ())?
        .build()
        .await
        .map_err(|_| ())?;
    proxy
        .on_applet_state_changed(true)
        .await
        .map_err(|_| ())?;
    let history = proxy.get_history().await.map_err(|_| ())?;
    let active_index = proxy.get_active_index().await.map_err(|_| ())?;
    let tracking = proxy.active().await.map_err(|_| ())?;
    Ok(DbusEvent::Refreshed {
        history,
        active_index,
        tracking,
    })
}

/// DBus I/O on a background thread; iced subscription only bridges events (never blocks UI).
pub fn subscription() -> Subscription<DbusEvent> {
    ensure_listener_thread();
    Subscription::run_with(TypeId::of::<DbusEvent>(), |_| {
        stream::channel(32, |mut output: iced_mpsc::Sender<DbusEvent>| async move {
            let rx = EVENT_RX
                .get()
                .expect("dbus listener")
                .lock()
                .expect("dbus listener lock")
                .as_ref()
                .expect("dbus listener running")
                .clone();

            loop {
                let rx = Arc::clone(&rx);
                let event = tokio::task::spawn_blocking(move || {
                    rx.lock()
                        .expect("dbus event receiver")
                        .recv_timeout(Duration::from_secs(30))
                })
                .await;

                let event = match event {
                    Ok(Ok(event)) => event,
                    Ok(Err(RecvTimeoutError::Timeout)) => continue,
                    _ => break,
                };

                if output.send(event).await.is_err() {
                    break;
                }
            }
        })
    })
}

fn ensure_listener_thread() {
    EVENT_RX.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel(64);
        thread::Builder::new()
            .name("cosmic-paste-dbus".into())
            .spawn(move || dbus_listener_thread(tx))
            .expect("spawn dbus listener");
        Mutex::new(Some(Arc::new(Mutex::new(rx))))
    });
}

fn dbus_listener_thread(event_tx: SyncSender<DbusEvent>) {
    let (show_tx, show_rx) = mpsc::sync_channel(8);
    spawn_show_history_socket(show_tx.clone());
    spawn_show_history_watcher(show_tx.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("dbus listener runtime");

    rt.block_on(async move {
        let mut backoff = Duration::from_millis(500);
        loop {
            match run_listener(&event_tx, &show_rx, &show_tx).await {
                Ok(()) => backoff = Duration::from_millis(500),
                Err(()) => {
                    mark_applet_absent().await;
                    let _ = event_tx.send(DbusEvent::Disconnected);
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
            }
        }
    });
}

struct AppletActivation {
    show_tx: SyncSender<()>,
}

impl AppletActivation {
    fn new(show_tx: SyncSender<()>) -> Self {
        Self { show_tx }
    }

    fn request_show_history(&self) {
        let _ = self.show_tx.try_send(());
    }
}

#[interface(name = "org.freedesktop.DbusActivation")]
impl AppletActivation {
    async fn activate(&mut self, _platform_data: HashMap<&str, Value<'_>>) {
        self.request_show_history();
    }

    async fn open(&mut self, _uris: Vec<&str>, _platform_data: HashMap<&str, Value<'_>>) {
        self.request_show_history();
    }

    async fn activate_action(
        &mut self,
        action_name: &str,
        _parameter: Vec<&str>,
        _platform_data: HashMap<&str, Value<'_>>,
    ) {
        if action_name == "show-history" {
            self.request_show_history();
        }
    }
}

fn spawn_show_history_socket(show_tx: mpsc::SyncSender<()>) {
    thread::Builder::new()
        .name("cosmic-paste-show-sock".into())
        .spawn(move || {
            let socket = match show_history_trigger::bind_socket() {
                Ok(socket) => socket,
                Err(err) => {
                    tracing::warn!("show-history socket unavailable: {err}");
                    return;
                }
            };
            let mut buf = [0u8; 8];
            while socket.recv(&mut buf).is_ok() {
                let _ = show_tx.try_send(());
            }
        })
        .ok();
}

fn spawn_show_history_watcher(show_tx: mpsc::SyncSender<()>) {
    let Some(path) = show_history_trigger::trigger_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    thread::Builder::new()
        .name("cosmic-paste-show-hist".into())
        .spawn(move || {
            use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

            let (tx, rx) = mpsc::channel();
            let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
                Ok(watcher) => watcher,
                Err(err) => {
                    tracing::warn!("show-history watcher unavailable: {err}");
                    return;
                }
            };
            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
            if let Some(parent) = path.parent() {
                let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
            }

            loop {
                match rx.recv() {
                    Ok(Ok(event)) => {
                        if event.paths.iter().any(|p| p == &path) {
                            let _ = show_tx.try_send(());
                        }
                    }
                    Ok(Err(err)) => tracing::warn!("show-history watcher error: {err}"),
                    Err(_) => break,
                }
            }
        })
        .ok();
}

async fn wait_show_history(show_rx: &Receiver<()>) {
    loop {
        match show_rx.try_recv() {
            Ok(()) => return,
            Err(StdTryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
            Err(StdTryRecvError::Disconnected) => {
                std::future::pending::<()>().await;
            }
        }
    }
}

async fn mark_applet_absent() {
    let Ok(conn) = zbus::Connection::session().await else {
        return;
    };
    let Ok(builder) = CosmicPasteProxy::builder(&conn).destination(BUS_NAME) else {
        return;
    };
    let Ok(builder) = builder.path(OBJECT_PATH) else {
        return;
    };
    let Ok(proxy) = builder.build().await else {
        return;
    };
    let _ = proxy.on_applet_state_changed(false).await;
}

async fn run_listener(
    event_tx: &SyncSender<DbusEvent>,
    show_rx: &Receiver<()>,
    show_tx: &SyncSender<()>,
) -> Result<(), ()> {
    let conn = zbus::Connection::session().await.map_err(|_| ())?;
    let tick = tokio::spawn({
        let conn = conn.clone();
        async move {
            loop {
                conn.executor().tick().await;
            }
        }
    });

    let result = run_listener_connected(&conn, event_tx, show_rx, show_tx).await;
    tick.abort();
    result
}

async fn run_listener_connected(
    conn: &zbus::Connection,
    event_tx: &SyncSender<DbusEvent>,
    show_rx: &Receiver<()>,
    show_tx: &SyncSender<()>,
) -> Result<(), ()> {
    let activation = AppletActivation::new(show_tx.clone());
    if !conn
        .object_server()
        .at(APPLET_OBJECT_PATH, activation)
        .await
        .map_err(|_| ())?
    {
        return Err(());
    }
    let _ = conn.request_name(APPLET_BUS_NAME).await;

    let proxy = CosmicPasteProxy::builder(conn)
        .destination(BUS_NAME)
        .map_err(|_| ())?
        .path(OBJECT_PATH)
        .map_err(|_| ())?
        .build()
        .await
        .map_err(|_| ())?;

    proxy
        .on_applet_state_changed(true)
        .await
        .map_err(|_| ())?;
    refresh(&proxy, event_tx).await?;

    let rule = zbus::MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(INTERFACE_NAME)
        .map_err(|_| ())?
        .path(OBJECT_PATH)
        .map_err(|_| ())?
        .build();

    let mut stream = MessageStream::for_match_rule(rule, conn, Some(32))
        .await
        .map_err(|_| ())?;

    loop {
        tokio::select! {
            biased;
            _ = wait_show_history(show_rx) => {
                let _ = event_tx.send(DbusEvent::ShowHistory);
            }
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(msg)) => msg,
                    Some(Err(_)) | None => return Err(()),
                };
                let member = msg
                    .header()
                    .member()
                    .map(|name| name.to_string());

                match member.as_deref() {
                    Some("Update") => {
                        refresh(&proxy, event_tx).await?;
                    }
                    Some("ActiveIndexChanged") => {
                        let active_index = match msg.body().deserialize::<(u32, u32)>() {
                            Ok((index, _)) => index,
                            Err(_) => proxy.get_active_index().await.map_err(|_| ())?,
                        };
                        let _ = event_tx.send(DbusEvent::ActiveIndexChanged { active_index });
                    }
                    Some("ShowHistory") => {
                        let _ = event_tx.send(DbusEvent::ShowHistory);
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn refresh(
    proxy: &CosmicPasteProxy<'_>,
    event_tx: &SyncSender<DbusEvent>,
) -> Result<(), ()> {
    let history = proxy.get_history().await.map_err(|_| ())?;
    let active_index = proxy.get_active_index().await.map_err(|_| ())?;
    let tracking = proxy.active().await.map_err(|_| ())?;
    event_tx
        .send(DbusEvent::Refreshed {
            history,
            active_index,
            tracking,
        })
        .map_err(|_| ())
}