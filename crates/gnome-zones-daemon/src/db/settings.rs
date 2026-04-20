// crates/gnome-zones-daemon/src/db/settings.rs
use crate::db::Database;
use crate::error::Result;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

pub fn get_setting(db: &Database, key: &str) -> Result<Option<String>> {
    let v: Option<String> = db.conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    ).optional()?;
    Ok(v)
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
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Typed accessor for integer settings with a default.
pub fn get_int(db: &Database, key: &str, default: i64) -> Result<i64> {
    Ok(get_setting(db, key)?
        .and_then(|s| s.parse().ok())
        .unwrap_or(default))
}

pub fn get_bool(db: &Database, key: &str, default: bool) -> Result<bool> {
    Ok(match get_setting(db, key)?.as_deref() {
        Some("1") | Some("true")  => true,
        Some("0") | Some("false") => false,
        _ => default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::temp_db;

    #[test]
    fn set_and_get() {
        let db = temp_db();
        set_setting(&db, "gap_px", "8").unwrap();
        assert_eq!(get_setting(&db, "gap_px").unwrap(), Some("8".into()));
    }

    #[test]
    fn set_overwrites() {
        let db = temp_db();
        set_setting(&db, "k", "a").unwrap();
        set_setting(&db, "k", "b").unwrap();
        assert_eq!(get_setting(&db, "k").unwrap(), Some("b".into()));
    }

    #[test]
    fn get_all_returns_every_row() {
        let db = temp_db();
        set_setting(&db, "a", "1").unwrap();
        set_setting(&db, "b", "2").unwrap();
        let all = get_all_settings(&db).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("a").unwrap(), "1");
    }

    #[test]
    fn get_int_falls_back_on_missing() {
        let db = temp_db();
        assert_eq!(get_int(&db, "gap_px", 8).unwrap(), 8);
        set_setting(&db, "gap_px", "12").unwrap();
        assert_eq!(get_int(&db, "gap_px", 8).unwrap(), 12);
    }

    #[test]
    fn get_bool_parses_common_forms() {
        let db = temp_db();
        set_setting(&db, "paused", "true").unwrap();
        assert!(get_bool(&db, "paused", false).unwrap());
        set_setting(&db, "paused", "0").unwrap();
        assert!(!get_bool(&db, "paused", true).unwrap());
    }
}
