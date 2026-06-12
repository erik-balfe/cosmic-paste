use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::{SinkExt, StreamExt};
use cosmic::iced::{stream, Subscription};
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::{BUS_NAME, INTERFACE_NAME, OBJECT_PATH};
use zbus::message::Type;
use zbus::MessageStream;

#[derive(Debug, Clone)]
pub enum DbusEvent {
    Refreshed {
        history: Vec<(String, String)>,
        active_index: u32,
        tracking: bool,
    },
    ShowHistory,
    Disconnected,
}

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
    let history = proxy.get_history().await.map_err(|_| ())?;
    let active_index = proxy.get_active_index().await.map_err(|_| ())?;
    let tracking = proxy.active().await.map_err(|_| ())?;
    Ok(DbusEvent::Refreshed {
        history,
        active_index,
        tracking,
    })
}

pub fn subscription() -> Subscription<DbusEvent> {
    Subscription::run(|| {
        stream::channel(32, |mut output: mpsc::Sender<DbusEvent>| async move {
            let mut backoff = std::time::Duration::from_millis(500);
            loop {
                match listen(&mut output).await {
                    Ok(()) => backoff = std::time::Duration::from_millis(500),
                    Err(()) => {
                        let _ = output.send(DbusEvent::Disconnected).await;
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(std::time::Duration::from_secs(30));
                    }
                }
            }
        })
    })
}

async fn listen(output: &mut mpsc::Sender<DbusEvent>) -> Result<(), ()> {
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
    refresh(&proxy, output).await?;

    let rule = zbus::MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(INTERFACE_NAME)
        .map_err(|_| ())?
        .path(OBJECT_PATH)
        .map_err(|_| ())?
        .sender(BUS_NAME)
        .map_err(|_| ())?
        .build();

    let mut stream = MessageStream::for_match_rule(rule, &conn, Some(32))
        .await
        .map_err(|_| ())?;

    while let Some(msg) = stream.next().await {
        let msg = msg.map_err(|_| ())?;
        let member = msg
            .header()
            .member()
            .map(|name| name.to_string());

        match member.as_deref() {
            Some("Update") | Some("ActiveIndexChanged") => {
                refresh(&proxy, output).await?;
            }
            Some("ShowHistory") => {
                let _ = output.send(DbusEvent::ShowHistory).await;
            }
            _ => {}
        }
    }

    let _ = proxy.on_applet_state_changed(false).await;
    Err(())
}

async fn refresh(
    proxy: &CosmicPasteProxy<'_>,
    output: &mut mpsc::Sender<DbusEvent>,
) -> Result<(), ()> {
    let history = proxy.get_history().await.map_err(|_| ())?;
    let active_index = proxy.get_active_index().await.map_err(|_| ())?;
    let tracking = proxy.active().await.map_err(|_| ())?;
    output
        .send(DbusEvent::Refreshed {
            history,
            active_index,
            tracking,
        })
        .await
        .map_err(|_| ())
}