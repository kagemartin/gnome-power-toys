use rusqlite::{params, Result};
use crate::db::Database;

const DEFAULT_EXCLUSIONS: &[&str] = &[
    "org.keepassxc.KeePassXC",
    "com.1password.1password",
    "com.bitwarden.desktop",
];

pub fn add_exclusion(db: &Database, app_id: &str) -> Result<()> {
    db.conn.execute(
        "INSERT OR IGNORE INTO exclusions (app_id) VALUES (?1)",
        params![app_id],
    )?;
    Ok(())
}

pub fn remove_exclusion(db: &Database, app_id: &str) -> Result<()> {
    db.conn.execute("DELETE FROM exclusions WHERE app_id = ?1", params![app_id])?;
    Ok(())
}

pub fn is_excluded(db: &Database, app_id: &str) -> Result<bool> {
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM exclusions WHERE app_id = ?1",
        params![app_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Seeds the default exclusion list if the table is empty.
pub fn seed_defaults(db: &Database) -> Result<()> {
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM exclusions", [], |r| r.get(0),
    )?;
    if count == 0 {
        for app_id in DEFAULT_EXCLUSIONS {
            add_exclusion(db, app_id)?;
        }
    }
    Ok(())
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
    fn excluded_app_is_detected() {
        let db = db();
        add_exclusion(&db, "org.keepassxc.KeePassXC").unwrap();
        assert!(is_excluded(&db, "org.keepassxc.KeePassXC").unwrap());
        assert!(!is_excluded(&db, "org.gnome.gedit").unwrap());
    }

    #[test]
    fn remove_exclusion_works() {
        let db = db();
        add_exclusion(&db, "com.1password.1password").unwrap();
        remove_exclusion(&db, "com.1password.1password").unwrap();
        assert!(!is_excluded(&db, "com.1password.1password").unwrap());
    }

    #[test]
    fn seed_defaults_adds_known_managers() {
        let db = db();
        seed_defaults(&db).unwrap();
        assert!(is_excluded(&db, "org.keepassxc.KeePassXC").unwrap());
        assert!(is_excluded(&db, "com.1password.1password").unwrap());
        assert!(is_excluded(&db, "com.bitwarden.desktop").unwrap());
    }
}
