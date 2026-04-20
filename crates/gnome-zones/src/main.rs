mod activator;
mod app;
mod dbus;
mod editor;
mod error;
mod overlay;
mod panel;

use clap::Parser;
use futures_util::StreamExt;
use gtk4::prelude::*;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = app::Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let _guard = rt.enter();

    let proxy = rt
        .block_on(dbus::connect())
        .expect("failed to connect to org.gnome.Zones — is gnome-zones-daemon running?");

    let application = app::build_app();
    let rt_handle = rt.handle().clone();

    application.connect_activate(move |app| {
        // Three modes:
        //   --editor     → open editor on the chosen monitor, exit when closed
        //   --activator  → open activator on the chosen monitor, exit when dismissed
        //   (default)    → panel mode: background process with tray + signal subscriptions

        if cli.editor {
            let app_c = app.clone();
            let proxy_c = proxy.clone();
            let monitor_opt = cli.monitor.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                let monitor_key = resolve_monitor_key(&proxy_c, monitor_opt).await;
                editor::show(&app_c, proxy_c, monitor_key);
            });
            return;
        }

        if cli.activator {
            let app_c = app.clone();
            let proxy_c = proxy.clone();
            let monitor_opt = cli.monitor.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                let monitor_key = resolve_monitor_key(&proxy_c, monitor_opt).await;
                let paused = is_paused(&proxy_c).await;
                activator::show(&app_c, proxy_c, monitor_key, paused);
            });
            return;
        }

        run_panel_mode(app, proxy.clone(), rt_handle.clone());
    });

    application.run();
    drop(rt);
}

/// Resolve the preferred monitor_key: the CLI-provided value, else the daemon's
/// primary monitor, else the first listed monitor, else an empty string (the
/// daemon treats an empty key as "primary" in most codepaths).
async fn resolve_monitor_key(
    proxy: &dbus::ZonesProxy<'static>,
    preferred: Option<String>,
) -> String {
    if let Some(k) = preferred {
        return k;
    }
    let monitors = proxy.list_monitors().await.unwrap_or_default();
    monitors
        .iter()
        .find(|m| m.is_primary)
        .map(|m| m.monitor_key.clone())
        .or_else(|| monitors.first().map(|m| m.monitor_key.clone()))
        .unwrap_or_default()
}

/// Read the daemon's `paused` setting. Any error or missing key returns `false`.
async fn is_paused(proxy: &dbus::ZonesProxy<'static>) -> bool {
    let s = proxy.get_settings().await.unwrap_or_default();
    matches!(s.get("paused").map(String::as_str), Some("1") | Some("true"))
}

/// Panel (default) mode: spawn the tray, subscribe to daemon signals, and
/// route tray events and signals back to the GTK main context.
fn run_panel_mode(
    app: &gtk4::Application,
    proxy: dbus::ZonesProxy<'static>,
    rt_handle: tokio::runtime::Handle,
) {
    // A hidden 1x1 holder window keeps GtkApplication alive while the panel
    // runs in the background. Without a registered window, `application.run()`
    // exits as soon as `connect_activate` returns. We never present it.
    let hold = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(1)
        .default_height(1)
        .build();
    hold.set_visible(false);
    hold.set_hide_on_close(true);
    app.add_window(&hold);

    let app_weak = app.downgrade();

    gtk4::glib::MainContext::default().spawn_local(async move {
        // Initial state fetch
        let layouts = proxy.list_layouts().await.unwrap_or_default();
        let paused = is_paused(&proxy).await;

        // Spawn the tray on the tokio runtime.
        let (indicator, tray_rx) =
            match panel::Indicator::spawn(rt_handle.clone(), layouts, paused) {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::error!(error = %e, "panel: failed to spawn tray — running without it");
                    // Still keep the process alive via `hold` so signal
                    // subscriptions below continue to work.
                    let _hold = hold;
                    return;
                }
            };
        // Keep the Indicator alive via Rc; clones are captured by the tray
        // dispatcher + signal subscription futures below. The last clone in
        // this outer future ensures it outlives process startup.
        let indicator = std::rc::Rc::new(indicator);

        // --- Tray event dispatcher ----------------------------------------
        {
            let proxy = proxy.clone();
            let app_weak = app_weak.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                while let Ok(event) = tray_rx.recv().await {
                    let Some(app) = app_weak.upgrade() else {
                        break;
                    };
                    match event {
                        panel::TrayEvent::ShowActivator => {
                            let proxy = proxy.clone();
                            let app = app.clone();
                            gtk4::glib::MainContext::default().spawn_local(async move {
                                let mk = resolve_monitor_key(&proxy, None).await;
                                let paused = is_paused(&proxy).await;
                                activator::show(&app, proxy, mk, paused);
                            });
                        }
                        panel::TrayEvent::ShowEditor => {
                            let proxy = proxy.clone();
                            let app = app.clone();
                            gtk4::glib::MainContext::default().spawn_local(async move {
                                let mk = resolve_monitor_key(&proxy, None).await;
                                editor::show(&app, proxy, mk);
                            });
                        }
                        panel::TrayEvent::AssignLayout(id) => {
                            let proxy = proxy.clone();
                            gtk4::glib::MainContext::default().spawn_local(async move {
                                let mk = resolve_monitor_key(&proxy, None).await;
                                if mk.is_empty() {
                                    tracing::warn!("tray: no monitor to assign layout to");
                                    return;
                                }
                                if let Err(e) = proxy.assign_layout(&mk, id).await {
                                    tracing::warn!(
                                        error = %e,
                                        layout_id = id,
                                        "tray: assign failed"
                                    );
                                }
                            });
                        }
                        panel::TrayEvent::TogglePaused => {
                            let proxy = proxy.clone();
                            gtk4::glib::MainContext::default().spawn_local(async move {
                                if let Err(e) = proxy.toggle_paused().await {
                                    tracing::warn!(error = %e, "tray: toggle_paused failed");
                                }
                            });
                        }
                    }
                }
            });
        }

        // --- ActivatorRequested signal → open activator -------------------
        {
            let proxy = proxy.clone();
            let app_weak = app_weak.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                match proxy.receive_activator_requested().await {
                    Ok(mut stream) => {
                        while let Some(sig) = stream.next().await {
                            let Some(app) = app_weak.upgrade() else {
                                break;
                            };
                            if let Ok(args) = sig.args() {
                                let mk = args.monitor_key.clone();
                                let proxy = proxy.clone();
                                let app = app.clone();
                                gtk4::glib::MainContext::default().spawn_local(async move {
                                    let paused = is_paused(&proxy).await;
                                    activator::show(&app, proxy, mk, paused);
                                });
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to subscribe to ActivatorRequested");
                    }
                }
            });
        }

        // --- EditorRequested signal → open editor -------------------------
        {
            let proxy = proxy.clone();
            let app_weak = app_weak.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                match proxy.receive_editor_requested().await {
                    Ok(mut stream) => {
                        while let Some(sig) = stream.next().await {
                            let Some(app) = app_weak.upgrade() else {
                                break;
                            };
                            if let Ok(args) = sig.args() {
                                let mk = args.monitor_key.clone();
                                let proxy = proxy.clone();
                                let app = app.clone();
                                gtk4::glib::MainContext::default().spawn_local(async move {
                                    editor::show(&app, proxy, mk);
                                });
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to subscribe to EditorRequested");
                    }
                }
            });
        }

        // --- PausedChanged signal → update tray icon ----------------------
        {
            let proxy = proxy.clone();
            let indicator = indicator.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                match proxy.receive_paused_changed().await {
                    Ok(mut stream) => {
                        while let Some(sig) = stream.next().await {
                            if let Ok(args) = sig.args() {
                                indicator.set_paused(args.paused);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to subscribe to PausedChanged");
                    }
                }
            });
        }

        // --- LayoutsChanged signal → refresh tray layout submenu ----------
        {
            let proxy = proxy.clone();
            let indicator = indicator.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                match proxy.receive_layouts_changed().await {
                    Ok(mut stream) => {
                        while stream.next().await.is_some() {
                            match proxy.list_layouts().await {
                                Ok(layouts) => indicator.set_layouts(layouts),
                                Err(e) => {
                                    tracing::warn!(error = %e, "failed to refresh layouts");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to subscribe to LayoutsChanged");
                    }
                }
            });
        }

        // Keep the hidden holder window and the last Indicator reference alive
        // for the lifetime of this outer future. The window is also owned by
        // the GtkApplication via `app.add_window(&hold)`, but capturing it
        // here ensures the borrow-checker ties its lifetime to the spawned
        // subscriptions.
        let _hold = hold;
        let _keepalive = indicator;
    });
}
