use rusqlite::{params, Result};
use crate::config::Config;
use crate::db::Database;

pub fn run_retention(db: &Database, config: &Config) -> Result<()> {
    let cutoff = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64)
        - (config.retention_days as i64 * 86400);

    db.conn.execute(
        "DELETE FROM clips WHERE pinned = 0 AND created_at < ?1",
        params![cutoff],
    )?;

    db.conn.execute(
        "DELETE FROM clips WHERE pinned = 0 AND id NOT IN (
             SELECT id FROM clips WHERE pinned = 0
             ORDER BY created_at DESC
             LIMIT ?1
         )",
        params![config.retention_count as i64],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::{clips::{insert_clip, get_history, set_pinned}, Database};
    use tempfile::NamedTempFile;

    fn db() -> Database {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap()
    }

    fn default_config() -> Config {
        Config {
            retention_days: 7,
            retention_count: 100,
            shortcut_key: "Super+V".to_string(),
            incognito: false,
        }
    }

    #[test]
    fn count_limit_deletes_oldest_unpinned() {
        let db = db();
        for i in 0..105i64 {
            insert_clip(&db, format!("clip {}", i).as_bytes(), "text/plain", None, None).unwrap();
        }
        let cfg = Config { retention_count: 100, ..default_config() };
        run_retention(&db, &cfg).unwrap();
        let remaining = get_history(&db, "", "", 0, 200).unwrap();
        assert_eq!(remaining.len(), 100);
    }

    #[test]
    fn pinned_clips_are_exempt_from_count_limit() {
        let db = db();
        for i in 0..105i64 {
            let id = insert_clip(&db, format!("clip {}", i).as_bytes(), "text/plain", None, None).unwrap();
            if i < 5 {
                set_pinned(&db, id, true).unwrap();
            }
        }
        let cfg = Config { retention_count: 100, ..default_config() };
        run_retention(&db, &cfg).unwrap();
        let remaining = get_history(&db, "", "", 0, 200).unwrap();
        let pinned: Vec<_> = remaining.iter().filter(|c| c.pinned).collect();
        assert_eq!(pinned.len(), 5);
        let unpinned: Vec<_> = remaining.iter().filter(|c| !c.pinned).collect();
        assert_eq!(unpinned.len(), 100);
    }

    #[test]
    fn age_limit_deletes_old_clips() {
        let db = db();
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64) - (10 * 86400);
        db.conn.execute(
            "INSERT INTO clips (content, content_type, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![b"old clip".to_vec(), "text/plain", old_ts],
        ).unwrap();
        insert_clip(&db, b"new clip", "text/plain", None, None).unwrap();
        let cfg = Config { retention_days: 7, ..default_config() };
        run_retention(&db, &cfg).unwrap();
        let remaining = get_history(&db, "", "", 0, 100).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].preview.as_deref(), None);
    }
}
