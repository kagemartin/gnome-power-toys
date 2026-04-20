use serde::{Deserialize, Serialize};
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

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{serialized::Context, to_bytes, Endian};

    fn ctx() -> Context {
        Context::new_dbus(Endian::Little, 0)
    }

    #[test]
    fn clip_summary_roundtrips_over_dbus() {
        let summary = ClipSummary {
            id: 42,
            content_type: "text/plain".to_string(),
            preview: "hello".to_string(),
            source_app: "gedit".to_string(),
            created_at: 1700000000,
            pinned: true,
            tags: vec!["work".to_string()],
        };
        let encoded = to_bytes(ctx(), &summary).unwrap();
        let (decoded, _): (ClipSummary, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.content_type, "text/plain");
        assert!(decoded.pinned);
        assert_eq!(decoded.tags, vec!["work"]);
    }

    #[test]
    fn clip_detail_roundtrips_over_dbus() {
        let detail = ClipDetail {
            id: 1,
            content_type: "image/png".to_string(),
            preview: "[Image]".to_string(),
            source_app: String::new(),
            created_at: 1700000001,
            pinned: false,
            tags: vec![],
            content: vec![0x89, 0x50, 0x4e, 0x47],
        };
        let encoded = to_bytes(ctx(), &detail).unwrap();
        let (decoded, _): (ClipDetail, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.content, vec![0x89, 0x50, 0x4e, 0x47]);
    }
}
