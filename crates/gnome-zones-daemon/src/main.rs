// crates/gnome-zones-daemon/src/main.rs
mod db;
mod dbus;
mod error;
mod hotkeys;
mod math;
mod model;
mod monitors;
mod presets;
mod snap;
mod window;

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::db::Database;
use crate::monitors::MutterMonitorService;
use crate::snap::state::WindowStateMap;
use crate::snap::SnapEngine;
use crate::window::shim::ShimMover;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> crate::error::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,gnome_zones_daemon=debug")))
        .init();

    tracing::info!("gnome-zones-daemon starting");

    // --- Persistence ---
    let mut db = Database::open_default()?;
    presets::seed(&mut db)?;
    let db = Arc::new(Mutex::new(db));

    // --- D-Bus session connection ---
    let session_conn = zbus::Connection::session().await?;

    // --- Services ---
    let monitor_svc: Arc<dyn monitors::MonitorService> =
        Arc::new(MutterMonitorService::new(&session_conn).await?);

    let mover: Arc<dyn window::WindowMover> =
        match ShimMover::new(&session_conn).await {
            Ok(m) => {
                tracing::info!("window mover: gnome-zones-mover shell extension");
                Arc::new(m)
            }
            Err(e) => {
                tracing::warn!("shim unavailable ({e}); falling back to MutterMover");
                Arc::new(window::mutter::MutterMover::new(&session_conn).await?)
            }
        };

    // --- Snap engine ---
    let states = Arc::new(WindowStateMap::new());
    let snap_engine = Arc::new(SnapEngine::new(
        db.clone(),
        monitor_svc.clone(),
        mover.clone(),
        states.clone(),
    ));

    // --- First-run hotkey registration ---
    {
        let db_guard = db.lock().await;
        if let Err(e) = hotkeys::stash_gnome_defaults(&db_guard) {
            tracing::warn!("could not stash GNOME defaults: {e}");
        }
    }
    if let Err(e) = hotkeys::register_custom_bindings() {
        tracing::warn!("could not register hotkeys: {e}");
    }

    // --- D-Bus service ---
    let service = Arc::new(
        dbus::run_service(db.clone(), snap_engine.clone(), monitor_svc.clone()).await?
    );

    // --- Monitor hot-plug watcher ---
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel();
    let _hotplug = monitors::spawn_hotplug_watcher(
        session_conn.clone(),
        db.clone(),
        monitor_svc.clone(),
        notify_tx,
    ).await?;

    // Forward reconciliation notifications to our user-facing D-Bus signal.
    {
        let service = service.clone();
        tokio::spawn(async move {
            while notify_rx.recv().await.is_some() {
                if let Err(e) = service.emit_monitors_changed().await {
                    tracing::warn!("MonitorsChanged emit failed: {e}");
                }
            }
        });
    }

    tracing::info!("gnome-zones-daemon ready");

    // Park the main task until shutdown. `service` is kept alive through the
    // spawned task's Arc clone AND this local binding below.
    let _keep_service = service;
    tokio::signal::ctrl_c().await?;
    tracing::info!("gnome-zones-daemon shutting down");
    Ok(())
}
