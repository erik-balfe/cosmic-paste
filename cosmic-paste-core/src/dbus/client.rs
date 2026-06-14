//! Async zbus proxy for `org.system76.CosmicPaste2`.

#[zbus::proxy(
    interface = "org.system76.CosmicPaste2",
    default_service = "org.system76.CosmicPaste",
    default_path = "/org/system76/CosmicPaste",
    assume_defaults = true
)]
pub trait CosmicPaste {
    async fn add(&self, text: &str) -> zbus::Result<()>;

    async fn get_history(&self) -> zbus::Result<Vec<(String, String)>>;

    async fn get_active_index(&self) -> zbus::Result<u32>;

    async fn set_active_index(&self, index: u32) -> zbus::Result<()>;

    async fn select_at_offset(&self, offset: i32) -> zbus::Result<String>;

    async fn select_at_index(&self, index: u32) -> zbus::Result<String>;

    async fn select(&self, uuid: &str) -> zbus::Result<()>;

    async fn track(&self, tracking_state: bool) -> zbus::Result<()>;

    async fn on_applet_state_changed(&self, state: bool) -> zbus::Result<()>;

    async fn delete(&self, uuid: &str) -> zbus::Result<()>;

    async fn empty_history(&self, name: &str) -> zbus::Result<()>;

    async fn get_history_name(&self) -> zbus::Result<String>;

    async fn get_history_size(&self, name: &str) -> zbus::Result<u32>;

    async fn list_histories(&self) -> zbus::Result<Vec<String>>;

    async fn reexecute(&self) -> zbus::Result<()>;

    async fn show_history(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn active_index(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn applet_present(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn portal_shortcuts_available(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    async fn update(&self, action: &str, target: &str, index: u32) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::CosmicPasteProxy;
    use crate::dbus::state::DaemonState;
    use crate::dbus::{BUS_NAME, OBJECT_PATH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    async fn spawn_test_service() -> (zbus::Connection, zbus::Connection, String) {
        let mut daemon = DaemonState::new_in_memory();
        daemon.ack_clipboard_writes = true;
        let service = daemon.service(crate::dbus::lifecycle::LifecycleHandle::detached());
        let bus_name = format!(
            "{BUS_NAME}.test{}.case{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let server = zbus::connection::Builder::session()
            .unwrap()
            .name(bus_name.as_str())
            .unwrap()
            .serve_at(OBJECT_PATH, service)
            .unwrap()
            .build()
            .await
            .unwrap();

        let client = zbus::connection::Builder::session()
            .unwrap()
            .build()
            .await
            .unwrap();

        (server, client, bus_name)
    }

    async fn proxy<'a>(
        connection: &'a zbus::Connection,
        bus_name: &'a str,
    ) -> CosmicPasteProxy<'a> {
        CosmicPasteProxy::builder(connection)
            .destination(bus_name)
            .unwrap()
            .path(OBJECT_PATH)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn get_active_index_defaults_to_zero() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;
        assert_eq!(proxy.get_active_index().await.unwrap(), 0);
        assert_eq!(proxy.active_index().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn get_history_starts_empty() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;
        assert!(proxy.get_history().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_then_get_history_round_trip() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;

        proxy.add("hello dbus").await.unwrap();
        let history = proxy.get_history().await.unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].1, "hello dbus");
        assert_eq!(proxy.get_active_index().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn on_applet_state_changed_updates_property() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;

        assert!(!proxy.applet_present().await.unwrap());
        proxy.on_applet_state_changed(true).await.unwrap();
        assert!(proxy.applet_present().await.unwrap());
    }

    #[tokio::test]
    async fn reexecute_signals_lifecycle() {
        let (lifecycle, mut lifecycle_rx) = crate::dbus::lifecycle::LifecycleHandle::pair();
        let service = DaemonState::new_in_memory().service(lifecycle);
        let bus_name = format!("{BUS_NAME}.Test{}.reexec", std::process::id());

        let _server = zbus::connection::Builder::session()
            .unwrap()
            .name(bus_name.as_str())
            .unwrap()
            .serve_at(OBJECT_PATH, service)
            .unwrap()
            .build()
            .await
            .unwrap();

        let client = zbus::connection::Builder::session()
            .unwrap()
            .build()
            .await
            .unwrap();

        let proxy = CosmicPasteProxy::builder(&client)
            .destination(bus_name.as_str())
            .unwrap()
            .path(OBJECT_PATH)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.reexecute().await.unwrap();
        lifecycle_rx.changed().await.unwrap();
        assert_eq!(*lifecycle_rx.borrow(), crate::dbus::lifecycle::ShutdownReason::Reexecute);
    }

    #[tokio::test]
    async fn select_moves_item_to_front() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;

        proxy.add("older").await.unwrap();
        proxy.add("newer").await.unwrap();
        let history = proxy.get_history().await.unwrap();
        let older_uuid = history
            .iter()
            .find(|(_, text)| text == "older")
            .map(|(uuid, _)| uuid.clone())
            .expect("older entry");

        proxy.select(&older_uuid).await.unwrap();

        let history = proxy.get_history().await.unwrap();
        assert_eq!(history[0].1, "older");
        assert_eq!(history[1].1, "newer");
        assert_eq!(proxy.get_active_index().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn version_property_matches_crate() {
        let (_server, client, bus_name) = spawn_test_service().await;
        let proxy = proxy(&client, &bus_name).await;
        assert_eq!(
            proxy.version().await.unwrap(),
            env!("CARGO_PKG_VERSION").to_owned()
        );
    }
}