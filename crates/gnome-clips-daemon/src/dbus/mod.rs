pub mod interface;
pub mod types;

use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use zbus::ConnectionBuilder;

use crate::db::Database;
use crate::error::Result;
use interface::ClipsInterface;

pub enum DaemonEvent {
    ClipAdded(types::ClipSummary),
    ClipDeleted(i64),
    ClipUpdated(types::ClipSummary),
    IncognitoChanged(bool),
}

pub async fn run_service(
    db: Arc<Mutex<Database>>,
    incognito_rx: watch::Receiver<bool>,
    mut events: mpsc::Receiver<DaemonEvent>,
) -> Result<()> {
    let iface = ClipsInterface {
        db: db.clone(),
        incognito: incognito_rx,
    };

    let conn = ConnectionBuilder::session()?
        .name("org.gnome.Clips")?
        .serve_at("/org/gnome/Clips", iface)?
        .build()
        .await?;

    let object_server = conn.object_server();

    loop {
        let Some(event) = events.recv().await else { break };

        let iface_ref = object_server
            .interface::<_, ClipsInterface>("/org/gnome/Clips")
            .await?;
        let ctx = iface_ref.signal_context();

        match event {
            DaemonEvent::ClipAdded(clip) => {
                ClipsInterface::clip_added(ctx, clip).await?;
            }
            DaemonEvent::ClipDeleted(id) => {
                ClipsInterface::clip_deleted(ctx, id).await?;
            }
            DaemonEvent::ClipUpdated(clip) => {
                ClipsInterface::clip_updated(ctx, clip).await?;
            }
            DaemonEvent::IncognitoChanged(enabled) => {
                ClipsInterface::incognito_changed(ctx, enabled).await?;
            }
        }
    }

    Ok(())
}
