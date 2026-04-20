use rusqlite::{Connection, Result as SqlResult};
use std::path::Path;

pub mod clips;
pub mod exclusions;
pub mod settings;
pub mod tags;
mod schema;

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
            .ok_or_else(|| crate::error::Error::Clipboard("cannot find data dir".into()))?;
        data_dir.push("gnome-clips");
        std::fs::create_dir_all(&data_dir)?;
        data_dir.push("history.db");
        Ok(Self::open(&data_dir)?)
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(schema::CREATE_CLIPS)?;
        self.conn.execute_batch(schema::CREATE_TAGS)?;
        self.conn.execute_batch(schema::CREATE_CLIP_TAGS)?;
        self.conn.execute_batch(schema::CREATE_SETTINGS)?;
        self.conn.execute_batch(schema::CREATE_EXCLUSIONS)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_db() -> Database {
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
        assert!(tables.contains(&"clips".to_string()));
        assert!(tables.contains(&"tags".to_string()));
        assert!(tables.contains(&"clip_tags".to_string()));
        assert!(tables.contains(&"settings".to_string()));
        assert!(tables.contains(&"exclusions".to_string()));
    }
}
