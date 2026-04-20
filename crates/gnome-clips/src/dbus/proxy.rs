// zbus proxy for org.gnome.Clips — mirrors the interface defined in the
// daemon's dbus/interface.rs. Types are redeclared here (they must match the
// daemon's wire format; a roundtrip test in the daemon crate pins the layout).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zbus::proxy;
use zbus::zvariant::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ClipSummary {
    pub id: i64,
    pub content_type: String,
    pub preview: String,
    pub source_app: String,
    pub created_at: i64,
    pub pinned: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ClipDetail {
    pub id: i64,
    pub content_type: String,
    pub preview: String,
    pub source_app: String,
    pub created_at: i64,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub content: Vec<u8>,
}

#[proxy(
    interface = "org.gnome.Clips",
    default_service = "org.gnome.Clips",
    default_path = "/org/gnome/Clips"
)]
pub trait Clips {
    async fn get_history(
        &self,
        filter: &str,
        search: &str,
        offset: u32,
        limit: u32,
    ) -> zbus::Result<Vec<ClipSummary>>;

    async fn get_clip(&self, id: i64) -> zbus::Result<ClipDetail>;
    async fn delete_clip(&self, id: i64) -> zbus::Result<()>;
    async fn set_pinned(&self, id: i64, pinned: bool) -> zbus::Result<()>;
    async fn add_tag(&self, id: i64, tag: &str) -> zbus::Result<()>;
    async fn remove_tag(&self, id: i64, tag: &str) -> zbus::Result<()>;
    async fn get_settings(&self) -> zbus::Result<HashMap<String, String>>;
    async fn set_setting(&self, key: &str, value: &str) -> zbus::Result<()>;
    async fn add_exclusion(&self, app_id: &str) -> zbus::Result<()>;
    async fn remove_exclusion(&self, app_id: &str) -> zbus::Result<()>;
    async fn set_incognito(&self, enabled: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn is_incognito(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    async fn clip_added(&self, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn clip_deleted(&self, id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn clip_updated(&self, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn incognito_changed(&self, enabled: bool) -> zbus::Result<()>;
}
