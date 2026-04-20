use rusqlite::{params, OptionalExtension, Result};
use crate::db::Database;
use std::collections::HashMap;

pub fn get_setting(db: &Database, key: &str) -> Result<Option<String>> {
    db.conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
}

pub fn set_setting(db: &Database, key: &str, value: &str) -> Result<()> {
    db.conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_all_settings(db: &Database) -> Result<HashMap<String, String>> {
    let mut stmt = db.conn.prepare("SELECT key, value FROM settings")?;
    let map = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<HashMap<_, _>>>()?;
    Ok(map)
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
    fn set_and_get_setting() {
        let db = db();
        set_setting(&db, "retention_days", "14").unwrap();
        assert_eq!(get_setting(&db, "retention_days").unwrap(), Some("14".to_string()));
    }

    #[test]
    fn missing_key_returns_none() {
        let db = db();
        assert_eq!(get_setting(&db, "nonexistent").unwrap(), None);
    }

    #[test]
    fn overwrite_setting() {
        let db = db();
        set_setting(&db, "retention_days", "7").unwrap();
        set_setting(&db, "retention_days", "30").unwrap();
        assert_eq!(get_setting(&db, "retention_days").unwrap(), Some("30".to_string()));
    }
}
