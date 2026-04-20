use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use zbus::interface;

use crate::clipboard::writer::ClipboardWriter;
use crate::db::{
    clips::{delete_clip, get_clip, get_history, set_pinned, touch_clip},
    exclusions::{add_exclusion, remove_exclusion},
    settings::{get_all_settings, set_setting},
    tags::{add_tag, get_clip_tags, remove_tag},
    Database,
};
use crate::dbus::types::{ClipDetail, ClipSummary};

pub struct ClipsInterface {
    pub db: Arc<Mutex<Database>>,
    /// Receives incognito state. `true` = incognito on.
    pub incognito: watch::Receiver<bool>,
    /// Broadcasts incognito state changes to all subscribers.
    pub incognito_tx: watch::Sender<bool>,
    /// Writes stored clips back to the system clipboard.
    pub writer: Arc<ClipboardWriter>,
}

fn map_err(e: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(e.to_string())
}

fn to_summary(db: &Database, row: &crate::db::clips::ClipRow) -> zbus::fdo::Result<ClipSummary> {
    let tags = get_clip_tags(db, row.id).map_err(map_err)?;
    Ok(ClipSummary {
        id: row.id,
        content_type: row.content_type.clone(),
        preview: row.preview.clone().unwrap_or_default(),
        source_app: row.source_app.clone().unwrap_or_default(),
        created_at: row.created_at,
        pinned: row.pinned,
        tags,
    })
}

#[interface(name = "org.gnome.Clips")]
impl ClipsInterface {
    async fn get_history(
        &self,
        filter: String,
        search: String,
        offset: u32,
        limit: u32,
    ) -> zbus::fdo::Result<Vec<ClipSummary>> {
        let db = self.db.lock().unwrap();
        let rows = get_history(&db, &filter, &search, offset, limit).map_err(map_err)?;
        rows.iter().map(|r| to_summary(&db, r)).collect()
    }

    async fn get_clip(&self, id: i64) -> zbus::fdo::Result<ClipDetail> {
        let db = self.db.lock().unwrap();
        let clip = get_clip(&db, id)
            .map_err(map_err)?
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("clip {} not found", id)))?;
        let tags = get_clip_tags(&db, id).map_err(map_err)?;
        Ok(ClipDetail {
            id: clip.id,
            content_type: clip.content_type,
            preview: clip.preview.unwrap_or_default(),
            source_app: clip.source_app.unwrap_or_default(),
            created_at: clip.created_at,
            pinned: clip.pinned,
            tags,
            content: clip.content,
        })
    }

    async fn delete_clip(&self, id: i64) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        delete_clip(&db, id).map_err(map_err)
    }

    async fn set_pinned(&self, id: i64, pinned: bool) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        set_pinned(&db, id, pinned).map_err(map_err)
    }

    async fn add_tag(&self, id: i64, tag: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        add_tag(&db, id, &tag).map_err(map_err)
    }

    async fn remove_tag(&self, id: i64, tag: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        remove_tag(&db, id, &tag).map_err(map_err)
    }

    async fn get_settings(&self) -> zbus::fdo::Result<std::collections::HashMap<String, String>> {
        let db = self.db.lock().unwrap();
        get_all_settings(&db).map_err(map_err)
    }

    async fn set_setting(&self, key: String, value: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        set_setting(&db, &key, &value).map_err(map_err)
    }

    async fn add_exclusion(&self, app_id: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        add_exclusion(&db, &app_id).map_err(map_err)
    }

    async fn remove_exclusion(&self, app_id: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        remove_exclusion(&db, &app_id).map_err(map_err)
    }

    #[zbus(property)]
    async fn is_incognito(&self) -> bool {
        *self.incognito.borrow()
    }

    async fn set_incognito(&self, enabled: bool) -> zbus::fdo::Result<()> {
        let _ = self.incognito_tx.send(enabled);
        Ok(())
    }

    /// Reads the stored clip and writes its bytes to the system clipboard
    /// so the next Ctrl+V in any app produces them. Bumps the clip's
    /// `created_at` so it re-sorts to the top on the next list refresh,
    /// and emits `ClipUpdated` so subscribers see the new ordering.
    async fn paste(
        &self,
        #[zbus(signal_context)] ctx: zbus::SignalContext<'_>,
        id: i64,
    ) -> zbus::fdo::Result<()> {
        let clip = {
            let db = self.db.lock().unwrap();
            get_clip(&db, id)
                .map_err(map_err)?
                .ok_or_else(|| zbus::fdo::Error::Failed(format!("clip {id} not found")))?
        };
        self.writer
            .write(&clip.content, &clip.content_type)
            .await
            .map_err(map_err)?;

        // Mark this clip most-recent and publish the new state.
        let summary = {
            let db = self.db.lock().unwrap();
            touch_clip(&db, id).map_err(map_err)?;
            let row = get_clip(&db, id)
                .map_err(map_err)?
                .ok_or_else(|| zbus::fdo::Error::Failed(format!("clip {id} disappeared")))?;
            let tags = get_clip_tags(&db, id).map_err(map_err)?;
            ClipSummary {
                id: row.id,
                content_type: row.content_type,
                preview: row.preview.unwrap_or_default(),
                source_app: row.source_app.unwrap_or_default(),
                created_at: row.created_at,
                pinned: row.pinned,
                tags,
            }
        };
        Self::clip_updated(&ctx, summary).await.ok();
        Ok(())
    }

    #[zbus(signal)]
    pub async fn clip_added(ctx: &zbus::SignalContext<'_>, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn clip_deleted(ctx: &zbus::SignalContext<'_>, id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn clip_updated(ctx: &zbus::SignalContext<'_>, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn incognito_changed(ctx: &zbus::SignalContext<'_>, enabled: bool) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{clips::insert_clip, Database};
    use std::sync::{Arc, Mutex};
    use tempfile::NamedTempFile;
    use tokio::sync::watch;

    async fn test_iface() -> ClipsInterface {
        let f = NamedTempFile::new().unwrap();
        let db = Database::open(f.path()).unwrap();
        insert_clip(&db, b"hello", "text/plain", Some("hello"), Some("gedit")).unwrap();
        let (tx, rx) = watch::channel(false);
        let last_hash = Arc::new(tokio::sync::Mutex::new(None));
        let writer = Arc::new(ClipboardWriter::new(
            crate::clipboard::Backend::Noop,
            last_hash,
        ));
        ClipsInterface {
            db: Arc::new(Mutex::new(db)),
            incognito: rx,
            incognito_tx: tx,
            writer,
        }
    }

    #[tokio::test]
    async fn get_history_returns_clips() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].content_type, "text/plain");
    }

    #[tokio::test]
    async fn get_clip_returns_content() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        let detail = iface.get_clip(id).await.unwrap();
        assert_eq!(detail.content, b"hello");
    }

    #[tokio::test]
    async fn delete_clip_removes_it() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        iface.delete_clip(id).await.unwrap();
        let clips_after = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert_eq!(clips_after.len(), 0);
    }

    #[tokio::test]
    async fn set_pinned_toggles_flag() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        iface.set_pinned(id, true).await.unwrap();
        let clips_after = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert!(clips_after[0].pinned);
    }
}
