mod app;
mod dbus;
mod error;
mod panel;
mod shortcut;
mod util;

use std::cell::RefCell;
use std::rc::Rc;

use futures_util::StreamExt;
use gtk4::glib;
use libadwaita::prelude::*;

use app::window::ClipsWindow;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Remove any stale media-keys entry we may have written in older builds
    // before we switched to a shell extension. Idempotent, no-op if absent.
    shortcut::cleanup_legacy_media_keys_entry();

    let proxy = glib::MainContext::default()
        .block_on(dbus::connect())
        .expect("failed to connect to gnome-clips-daemon — is the daemon running?");

    let application = app::build_app();

    // Built lazily on the first `activate`; subsequent activations toggle.
    let state: Rc<RefCell<Option<AppState>>> = Rc::new(RefCell::new(None));

    application.connect_activate(move |app| {
        if let Some(s) = state.borrow().as_ref() {
            s.window.toggle();
            return;
        }
        let window = Rc::new(ClipsWindow::new(app, proxy.clone()));
        let (tray, panel_rx) = panel::indicator::spawn();
        wire_tray(&proxy, &tray, &window, panel_rx);
        window.show();
        *state.borrow_mut() = Some(AppState { window, _tray: tray });
    });

    application.run();
}

struct AppState {
    window: Rc<ClipsWindow>,
    // Kept alive for the duration of the process; dropping would stop the
    // ksni thread.
    _tray: ksni::Handle<panel::indicator::ClipsTray>,
}

fn wire_tray(
    proxy: &dbus::ClipsProxy<'static>,
    tray: &ksni::Handle<panel::indicator::ClipsTray>,
    window: &Rc<ClipsWindow>,
    panel_rx: async_channel::Receiver<panel::indicator::PanelEvent>,
) {
    // Mirror daemon incognito state onto the tray icon.
    {
        let proxy = proxy.clone();
        let handle = tray.clone();
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

    // Pump tray menu events into the GTK main loop.
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
}
