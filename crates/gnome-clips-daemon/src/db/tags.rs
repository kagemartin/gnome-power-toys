use rusqlite::{params, Result};
use crate::db::Database;

/// Adds a tag to a clip. Creates the tag if it doesn't exist. Idempotent.
pub fn add_tag(db: &Database, clip_id: i64, tag: &str) -> Result<()> {
    db.conn.execute(
        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
        params![tag],
    )?;
    let tag_id: i64 = db.conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![tag],
        |r| r.get(0),
    )?;
    db.conn.execute(
        "INSERT OR IGNORE INTO clip_tags (clip_id, tag_id) VALUES (?1, ?2)",
        params![clip_id, tag_id],
    )?;
    Ok(())
}

/// Removes a tag from a clip. Leaves the tag row in `tags` (may be shared).
pub fn remove_tag(db: &Database, clip_id: i64, tag: &str) -> Result<()> {
    db.conn.execute(
        "DELETE FROM clip_tags WHERE clip_id = ?1
         AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
        params![clip_id, tag],
    )?;
    Ok(())
}

pub fn get_clip_tags(db: &Database, clip_id: i64) -> Result<Vec<String>> {
    let mut stmt = db.conn.prepare(
        "SELECT t.name FROM tags t
         JOIN clip_tags ct ON ct.tag_id = t.id
         WHERE ct.clip_id = ?1
         ORDER BY t.name",
    )?;
    let tags = stmt
        .query_map(params![clip_id], |r| r.get(0))?
        .collect::<Result<Vec<String>>>()?;
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{clips::insert_clip, Database};
    use tempfile::NamedTempFile;

    fn db() -> Database {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap()
    }

    #[test]
    fn add_and_get_tag() {
        let db = db();
        let clip_id = insert_clip(&db, b"x", "text/plain", None, None).unwrap();
        add_tag(&db, clip_id, "work").unwrap();
        let tags = get_clip_tags(&db, clip_id).unwrap();
        assert_eq!(tags, vec!["work".to_string()]);
    }

    #[test]
    fn remove_tag_works() {
        let db = db();
        let clip_id = insert_clip(&db, b"x", "text/plain", None, None).unwrap();
        add_tag(&db, clip_id, "work").unwrap();
        add_tag(&db, clip_id, "personal").unwrap();
        remove_tag(&db, clip_id, "work").unwrap();
        let tags = get_clip_tags(&db, clip_id).unwrap();
        assert_eq!(tags, vec!["personal".to_string()]);
    }

    #[test]
    fn duplicate_tag_is_idempotent() {
        let db = db();
        let clip_id = insert_clip(&db, b"x", "text/plain", None, None).unwrap();
        add_tag(&db, clip_id, "work").unwrap();
        add_tag(&db, clip_id, "work").unwrap();
        let tags = get_clip_tags(&db, clip_id).unwrap();
        assert_eq!(tags.len(), 1);
    }
}
