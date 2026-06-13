//! Ask the panel applet to open its history popup via `org.freedesktop.DbusActivation`.

pub const APPLET_BUS_NAME: &str = "com.system76.CosmicPaste.Applet";
pub const APPLET_OBJECT_PATH: &str = "/com/system76/CosmicPaste/Applet";

/// Invoke `ActivateAction(show-history)` on the running panel applet, if any.
pub async fn activate_show_history() {
    let Ok(conn) = zbus::Connection::session().await else {
        return;
    };

    use zbus::zvariant::OwnedValue;
    #[zbus::proxy(
        interface = "org.freedesktop.DbusActivation",
        default_service = "com.system76.CosmicPaste.Applet",
        default_path = "/com/system76/CosmicPaste/Applet",
        assume_defaults = true
    )]
    trait DbusActivation {
        fn activate_action(
            &self,
            action_name: &str,
            parameter: Vec<&str>,
            platform_data: std::collections::HashMap<&str, OwnedValue>,
        ) -> zbus::Result<()>;
    }

    let proxy = match DbusActivationProxy::new(&conn).await {
        Ok(proxy) => proxy,
        Err(err) => {
            tracing::debug!("applet dbus activation unavailable: {err}");
            return;
        }
    };

    if let Err(err) = proxy
        .activate_action("show-history", Vec::new(), std::collections::HashMap::new())
        .await
    {
        tracing::debug!("applet show-history activation failed: {err}");
    }
}