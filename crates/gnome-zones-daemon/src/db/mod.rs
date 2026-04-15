// crates/gnome-zones-daemon/src/db/mod.rs
use rusqlite::{Connection, Result as SqlResult};
use std::path::Path;

pub mod layouts;
pub mod monitors;
mod schema;
pub mod settings;

pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> crate::error::Result<Self> {
        let mut data_dir = dirs::data_dir()
            .ok_or_else(|| crate::error::Error::Config("cannot find data dir".into()))?;
        data_dir.push("gnome-zones");
        std::fs::create_dir_all(&data_dir)?;
        data_dir.push("zones.db");
        Ok(Self::open(&data_dir)?)
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(schema::CREATE_LAYOUTS)?;
        self.conn.execute_batch(schema::CREATE_ZONES)?;
        self.conn.execute_batch(schema::CREATE_MONITOR_ASSIGNMENTS)?;
        self.conn.execute_batch(schema::CREATE_SETTINGS)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    pub(crate) fn temp_db() -> Database {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap()
    }

    #[test]
    fn schema_creates_all_tables() {
        let db = temp_db();
        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(tables.contains(&"layouts".to_string()));
        assert!(tables.contains(&"zones".to_string()));
        assert!(tables.contains(&"monitor_assignments".to_string()));
        assert!(tables.contains(&"settings".to_string()));
    }

    #[test]
    fn foreign_keys_are_on() {
        let db = temp_db();
        let fk: i32 = db
            .conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }
}
