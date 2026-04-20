mod app;
mod dbus;
mod error;
mod panel;
mod shortcut;

use std::rc::Rc;

use futures_util::StreamExt;
use gtk4::glib;
use libadwaita::prelude::*;

use app::window::ClipsWindow;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    shortcut::register_shortcut("<Super>v");

    let proxy = glib::MainContext::default()
        .block_on(dbus::connect())
        .expect("failed to connect to gnome-clips-daemon — is the daemon running?");

    let application = app::build_app();

    application.connect_activate(move |app| {
        let window = Rc::new(ClipsWindow::new(app, proxy.clone()));

        let (tray_handle, panel_rx) = panel::indicator::spawn();

        // Mirror daemon incognito state onto the tray icon.
        {
            let proxy = proxy.clone();
            let handle = tray_handle.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(cur) = proxy.is_incognito().await {
                    handle.update(|t| t.incognito = cur);
                }
                if let Ok(mut stream) = proxy.receive_incognito_changed().await {
                    while let Some(sig) = stream.next().await {
                        if let Ok(enabled) = sig.args() {
                            handle.update(|t| t.incognito = enabled.enabled);
                        }
                    }
                }
            });
        }

        // Pump tray events into the GTK main loop.
        {
            let window = window.clone();
            let proxy = proxy.clone();
            glib::MainContext::default().spawn_local(async move {
                while let Ok(ev) = panel_rx.recv().await {
                    match ev {
                        panel::indicator::PanelEvent::Activate => window.toggle(),
                        panel::indicator::PanelEvent::ToggleIncognito => {
                            let current = proxy.is_incognito().await.unwrap_or(false);
                            let _ = proxy.set_incognito(!current).await;
                        }
                        panel::indicator::PanelEvent::Quit => std::process::exit(0),
                    }
                }
            });
        }

        window.show();
    });

    application.run();
}
