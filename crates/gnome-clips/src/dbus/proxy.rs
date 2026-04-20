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

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{serialized::Context, to_bytes, Endian};

    fn ctx() -> Context {
        Context::new_dbus(Endian::Little, 0)
    }

    // These mirror the daemon's roundtrip tests: if the wire format ever
    // drifts between daemon and UI, one of these will fail.

    #[test]
    fn clip_summary_roundtrips_over_dbus() {
        let summary = ClipSummary {
            id: 42,
            content_type: "text/plain".into(),
            preview: "hello".into(),
            source_app: "gedit".into(),
            created_at: 1_700_000_000,
            pinned: true,
            tags: vec!["work".into()],
        };
        let encoded = to_bytes(ctx(), &summary).unwrap();
        let (decoded, _): (ClipSummary, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.content_type, "text/plain");
        assert_eq!(decoded.preview, "hello");
        assert_eq!(decoded.source_app, "gedit");
        assert_eq!(decoded.created_at, 1_700_000_000);
        assert!(decoded.pinned);
        assert_eq!(decoded.tags, vec!["work"]);
    }

    #[test]
    fn clip_detail_roundtrips_over_dbus() {
        let detail = ClipDetail {
            id: 1,
            content_type: "image/png".into(),
            preview: "[Image]".into(),
            source_app: String::new(),
            created_at: 1_700_000_001,
            pinned: false,
            tags: vec![],
            content: vec![0x89, 0x50, 0x4e, 0x47],
        };
        let encoded = to_bytes(ctx(), &detail).unwrap();
        let (decoded, _): (ClipDetail, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.content, vec![0x89, 0x50, 0x4e, 0x47]);
        assert_eq!(decoded.content_type, "image/png");
        assert!(!decoded.pinned);
    }

    #[test]
    fn clip_summary_empty_strings_and_tags_roundtrip() {
        let summary = ClipSummary {
            id: 0,
            content_type: String::new(),
            preview: String::new(),
            source_app: String::new(),
            created_at: 0,
            pinned: false,
            tags: vec![],
        };
        let encoded = to_bytes(ctx(), &summary).unwrap();
        let (decoded, _): (ClipSummary, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.id, 0);
        assert!(decoded.tags.is_empty());
    }
}
