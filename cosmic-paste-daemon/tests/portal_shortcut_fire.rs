//! Portal integration tests for PR 7a (require session D-Bus + xdg-desktop-portal).

use std::time::Duration;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;

async fn portal_reachable() -> bool {
    GlobalShortcuts::new().await.is_ok()
}

#[tokio::test]
async fn portal_shortcut_bind_smoke() {
    if !portal_reachable().await {
        eprintln!("skip: org.freedesktop.portal.GlobalShortcuts not on session bus");
        return;
    }

    let Ok(proxy) = GlobalShortcuts::new().await else {
        eprintln!("skip: GlobalShortcuts proxy unavailable");
        return;
    };
    let Ok(session) = proxy.create_session().await else {
        eprintln!("skip: cannot create GlobalShortcuts session (no portal host?)");
        return;
    };
    let shortcut = NewShortcut::new(
        "show-history",
        "cosmic-paste PR7a bind smoke test",
    )
    .preferred_trigger(Some("<Ctrl>F11"));

    let Ok(bind_req) = proxy.bind_shortcuts(&session, &[shortcut], None).await else {
        eprintln!("skip: BindShortcuts request failed");
        return;
    };
    let response = bind_req.response();
    match response {
        Ok(bound) => {
            assert!(
                !bound.shortcuts().is_empty(),
                "expected at least one bound shortcut"
            );
        }
        Err(err) => {
            eprintln!("bind response not OK (permission dialog?): {err}");
        }
    }
}

#[tokio::test]
#[ignore = "manual on COSMIC: run with `just test-portal` and press Ctrl+F11"]
async fn portal_shortcut_fire() {
    if !portal_reachable().await {
        eprintln!("skip: portal not reachable");
        return;
    }

    let proxy = GlobalShortcuts::new().await.expect("portal");
    let session = proxy.create_session().await.expect("session");
    let shortcut = NewShortcut::new("show-history", "Press Ctrl+F11 now")
        .preferred_trigger(Some("<Ctrl>F11"));
    let bind_req = proxy
        .bind_shortcuts(&session, &[shortcut], None)
        .await
        .expect("bind request");
    bind_req
        .response()
        .expect("approve shortcut permission if prompted");

    let mut activated = proxy.receive_activated().await.expect("Activated stream");
    eprintln!("Waiting up to 30s for Ctrl+F11 (show-history)...");

    let fired = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(activation) = activated.next().await {
            if activation.shortcut_id() == "show-history" {
                return;
            }
        }
        panic!("Activated stream ended without show-history");
    })
    .await;

    assert!(fired.is_ok(), "timed out waiting for show-history activation");
}