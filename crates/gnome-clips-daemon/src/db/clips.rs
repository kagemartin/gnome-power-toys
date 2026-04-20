use rusqlite::{params, OptionalExtension, Result};
use crate::db::Database;

pub struct Clip {
    pub id: i64,
    pub content: Vec<u8>,
    pub content_type: String,
    pub preview: Option<String>,
    pub source_app: Option<String>,
    pub created_at: i64,
    pub pinned: bool,
}

pub struct ClipRow {
    pub id: i64,
    pub content_type: String,
    pub preview: Option<String>,
    pub source_app: Option<String>,
    pub created_at: i64,
    pub pinned: bool,
}

pub fn insert_clip(
    db: &Database,
    content: &[u8],
    content_type: &str,
    preview: Option<&str>,
    source_app: Option<&str>,
) -> Result<i64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    db.conn.execute(
        "INSERT INTO clips (content, content_type, preview, source_app, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![content, content_type, preview, source_app, now],
    )?;
    Ok(db.conn.last_insert_rowid())
}

pub fn get_clip(db: &Database, id: i64) -> Result<Option<Clip>> {
    db.conn
        .query_row(
            "SELECT id, content, content_type, preview, source_app, created_at, pinned
             FROM clips WHERE id = ?1 AND deleted = 0",
            params![id],
            |row| {
                Ok(Clip {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    content_type: row.get(2)?,
                    preview: row.get(3)?,
                    source_app: row.get(4)?,
                    created_at: row.get(5)?,
                    pinned: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .optional()
}

pub fn delete_clip(db: &Database, id: i64) -> Result<()> {
    db.conn.execute(
        "DELETE FROM clips WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn set_pinned(db: &Database, id: i64, pinned: bool) -> Result<()> {
    db.conn.execute(
        "UPDATE clips SET pinned = ?1 WHERE id = ?2",
        params![pinned as i64, id],
    )?;
    Ok(())
}

/// Bump `created_at` to now so this clip sorts to the top on the next
/// history fetch. Used after a successful paste so the chosen clip
/// becomes the most-recent entry.
pub fn touch_clip(db: &Database, id: i64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    db.conn.execute(
        "UPDATE clips SET created_at = ?1 WHERE id = ?2 AND deleted = 0",
        params![now, id],
    )?;
    Ok(())
}

/// filter: "" = all, "pinned" = pinned only, any MIME type prefix = filter by type
/// search: substring match on preview
pub fn get_history(
    db: &Database,
    filter: &str,
    search: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<ClipRow>> {
    let pinned_only = filter == "pinned";
    let type_filter = if filter.is_empty() || pinned_only { None } else { Some(filter) };
    let search_pat = if search.is_empty() {
        None
    } else {
        Some(format!("%{}%", search))
    };

    let mut sql = "SELECT id, content_type, preview, source_app, created_at, pinned
                   FROM clips WHERE deleted = 0".to_string();
    if pinned_only { sql.push_str(" AND pinned = 1"); }
    if let Some(t) = type_filter {
        if t.ends_with("/*") {
            sql.push_str(" AND content_type LIKE ?");
        } else {
            sql.push_str(" AND content_type = ?");
        }
    }
    if search_pat.is_some() {
        sql.push_str(" AND preview LIKE ?");
    }
    sql.push_str(" ORDER BY pinned DESC, created_at DESC LIMIT ? OFFSET ?");

    let mut stmt = db.conn.prepare(&sql)?;

    let type_bind: Option<String> = type_filter.map(|t| {
        if t.ends_with("/*") {
            format!("{}%", &t[..t.len() - 1])
        } else {
            t.to_string()
        }
    });

    let mut bindings: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(t) = type_bind {
        bindings.push(rusqlite::types::Value::Text(t));
    }
    if let Some(s) = search_pat {
        bindings.push(rusqlite::types::Value::Text(s));
    }
    bindings.push(rusqlite::types::Value::Integer(limit as i64));
    bindings.push(rusqlite::types::Value::Integer(offset as i64));

    let rows = stmt
        .query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            Ok(ClipRow {
                id: row.get(0)?,
                content_type: row.get(1)?,
                preview: row.get(2)?,
                source_app: row.get(3)?,
                created_at: row.get(4)?,
                pinned: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn db() -> Database {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap()
    }

    #[test]
    fn insert_and_retrieve_clip() {
        let db = db();
        let id = insert_clip(&db, b"hello world", "text/plain", Some("Hello…"), Some("gedit")).unwrap();
        let clip = get_clip(&db, id).unwrap().unwrap();
        assert_eq!(clip.content, b"hello world");
        assert_eq!(clip.content_type, "text/plain");
        assert_eq!(clip.preview.as_deref(), Some("Hello…"));
        assert_eq!(clip.source_app.as_deref(), Some("gedit"));
        assert!(!clip.pinned);
    }

    #[test]
    fn delete_clip_removes_row() {
        let db = db();
        let id = insert_clip(&db, b"bye", "text/plain", None, None).unwrap();
        delete_clip(&db, id).unwrap();
        assert!(get_clip(&db, id).unwrap().is_none());
    }

    #[test]
    fn set_pinned_toggles_flag() {
        let db = db();
        let id = insert_clip(&db, b"pin me", "text/plain", None, None).unwrap();
        set_pinned(&db, id, true).unwrap();
        let clip = get_clip(&db, id).unwrap().unwrap();
        assert!(clip.pinned);
        set_pinned(&db, id, false).unwrap();
        let clip = get_clip(&db, id).unwrap().unwrap();
        assert!(!clip.pinned);
    }

    #[test]
    fn touch_clip_bumps_created_at() {
        let db = db();
        let id = insert_clip(&db, b"old", "text/plain", None, None).unwrap();
        let before = get_clip(&db, id).unwrap().unwrap().created_at;
        // Rewind one clip's timestamp into the past so we can observe
        // touch moving it forward, independent of wall-clock resolution.
        db.conn
            .execute("UPDATE clips SET created_at = ?1 WHERE id = ?2", params![before - 500, id])
            .unwrap();
        touch_clip(&db, id).unwrap();
        let after = get_clip(&db, id).unwrap().unwrap().created_at;
        assert!(after > before - 500, "touch_clip did not bump created_at");
    }

    #[test]
    fn touch_clip_moves_row_to_top_of_history() {
        let db = db();
        let old = insert_clip(&db, b"old", "text/plain", None, None).unwrap();
        let newer = insert_clip(&db, b"newer", "text/plain", None, None).unwrap();
        // Separate the two timestamps so ordering is unambiguous. The
        // test can then compare "old < newer" vs "old > newer" rather
        // than dealing with same-second ties.
        db.conn
            .execute("UPDATE clips SET created_at = 1000 WHERE id = ?1", params![old])
            .unwrap();
        db.conn
            .execute("UPDATE clips SET created_at = 2000 WHERE id = ?1", params![newer])
            .unwrap();

        let ordered: Vec<i64> = get_history(&db, "", "", 0, 100)
            .unwrap()
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ordered.first(), Some(&newer));

        // touch_clip uses the real clock, which is definitely > 2000.
        touch_clip(&db, old).unwrap();
        let ordered: Vec<i64> = get_history(&db, "", "", 0, 100)
            .unwrap()
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ordered.first(), Some(&old), "touched clip should be first");
    }

    #[test]
    fn get_history_filters_by_content_type() {
        let db = db();
        insert_clip(&db, b"text", "text/plain", None, None).unwrap();
        insert_clip(&db, b"img", "image/png", None, None).unwrap();
        let results = get_history(&db, "image/png", "", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_type, "image/png");
    }

    #[test]
    fn get_history_search_filters_preview() {
        let db = db();
        insert_clip(&db, b"foo", "text/plain", Some("hello world"), None).unwrap();
        insert_clip(&db, b"bar", "text/plain", Some("goodbye"), None).unwrap();
        let results = get_history(&db, "", "hello", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].preview.as_deref(), Some("hello world"));
    }
}
