mod clipboard;
mod config;
mod db;
mod dbus;
mod error;
mod incognito;
mod preview;
mod retention;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use crate::clipboard::writer::ClipboardWriter;
use crate::config::Config;
use crate::db::{exclusions::seed_defaults, Database};
use crate::dbus::DaemonEvent;
use crate::incognito::IncognitoState;

#[tokio::main]
async fn main() -> error::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("gnome-clips-daemon starting");

    let database = Database::open_default()?;
    seed_defaults(&database)?;
    let config = Config::load(&database)?;
    let db = Arc::new(Mutex::new(database));

    let incognito = IncognitoState::new(config.incognito);
    let incognito_rx = incognito.subscribe();
    let incognito_tx = incognito.sender();

    let (event_tx, event_rx) = mpsc::channel::<DaemonEvent>(64);

    let (clip_tx, mut clip_rx) = mpsc::channel::<clipboard::ClipboardEvent>(64);
    let last_hash = Arc::new(tokio::sync::Mutex::new(None));
    let backend = match clipboard::Backend::detect() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "falling back to no-op clipboard backend");
            clipboard::Backend::Noop
        }
    };
    let writer = Arc::new(ClipboardWriter::new(backend.clone(), last_hash.clone()));
    clipboard::start_monitor(backend, last_hash, clip_tx).await;

    {
        let db = db.clone();
        let event_tx = event_tx.clone();
        let incognito_rx2 = incognito.subscribe();
        tokio::spawn(async move {
            while let Some(event) = clip_rx.recv().await {
                if *incognito_rx2.borrow() {
                    continue;
                }

                let result = {
                    let db = db.lock().unwrap();

                    if let Some(ref app) = event.source_app {
                        if db::exclusions::is_excluded(&db, app).unwrap_or(false) {
                            continue;
                        }
                    }

                    let preview = preview::generate_preview(&event.content, &event.content_type);
                    let id = match db::clips::insert_clip(
                        &db,
                        &event.content,
                        &event.content_type,
                        Some(&preview),
                        event.source_app.as_deref(),
                    ) {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!(error = %e, "failed to insert clip");
                            continue;
                        }
                    };

                    let tags = db::tags::get_clip_tags(&db, id).unwrap_or_default();
                    let clip = match db::clips::get_clip(&db, id) {
                        Ok(Some(c)) => c,
                        _ => continue,
                    };

                    let summary = dbus::types::ClipSummary {
                        id,
                        content_type: clip.content_type,
                        preview: clip.preview.unwrap_or_default(),
                        source_app: clip.source_app.unwrap_or_default(),
                        created_at: clip.created_at,
                        pinned: clip.pinned,
                        tags,
                    };
                    (id, summary)
                };

                let (id, summary) = result;
                let _ = event_tx.send(DaemonEvent::ClipAdded(summary)).await;
                info!(clip_id = id, "clip stored");
            }
        });
    }

    {
        let db = db.clone();
        let retention_days = config.retention_days;
        let retention_count = config.retention_count;
        tokio::spawn(async move {
            let cfg = Config {
                retention_days,
                retention_count,
                shortcut_key: String::new(),
                incognito: false,
            };
            retention::run_retention(&db.lock().unwrap(), &cfg).ok();
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            interval.tick().await;
            loop {
                interval.tick().await;
                retention::run_retention(&db.lock().unwrap(), &cfg).ok();
            }
        });
    }

    dbus::run_service(db, incognito_rx, incognito_tx, writer, event_rx).await?;

    Ok(())
}
