# gnome-clips Daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `gnome-clips-daemon`, a systemd user service that monitors the clipboard, persists history to SQLite, and exposes everything over D-Bus at `org.gnome.Clips`.

**Architecture:** A tokio-based async service with three concurrent subsystems — a clipboard monitor polling for changes, a SQLite database layer for persistence, and a zbus D-Bus service exposing the full `org.gnome.Clips` interface. The three communicate via `tokio::sync::mpsc` channels and shared `Arc<Mutex<Database>>`.

**Tech Stack:** Rust 2021, tokio 1, zbus 4 (D-Bus), rusqlite 0.31 (bundled SQLite), wl-clipboard-rs 0.8 (Wayland), x11-clipboard 0.9 (X11 fallback), tracing 0.1, sha2 0.10.

**Spec:** `docs/superpowers/specs/2026-04-14-gnome-clips-design.md`

**This is Plan 1 of 3.** Plan 2 covers the GTK4 UI (`gnome-clips`). Plan 3 covers packaging (Flatpak + .deb).

---

## File Structure

```
gnome-power-toys/
├── Cargo.toml                                  # workspace root
└── crates/
    └── gnome-clips-daemon/
        ├── Cargo.toml
        └── src/
            ├── main.rs                         # tokio entry point, wires all subsystems
            ├── error.rs                        # unified Error + Result types
            ├── config.rs                       # Config struct (retention_days, retention_count, shortcut_key)
            ├── db/
            │   ├── mod.rs                      # Database struct, open(), migrate()
            │   ├── schema.rs                   # SQL DDL constants
            │   ├── clips.rs                    # insert_clip(), get_history(), get_clip(), delete_clip(), set_pinned()
            │   ├── tags.rs                     # add_tag(), remove_tag(), get_clip_tags()
            │   ├── settings.rs                 # get_setting(), set_setting(), get_all_settings()
            │   └── exclusions.rs               # add_exclusion(), remove_exclusion(), is_excluded()
            ├── preview.rs                      # generate_preview(content: &[u8], content_type: &str) -> String
            ├── retention.rs                    # run_retention(db, config) — hourly cleanup
            ├── clipboard/
            │   ├── mod.rs                      # ClipboardEvent struct, monitor() async fn
            │   ├── wayland.rs                  # poll_wayland() — wl-clipboard-rs
            │   └── x11.rs                      # poll_x11() — x11-clipboard
            └── dbus/
                ├── mod.rs                      # run_service(db, incognito_rx) — registers on session bus
                ├── types.rs                    # ClipSummary, ClipDetail — zbus-serializable structs
                └── interface.rs                # ClipsInterface — #[interface(name = "org.gnome.Clips")]
```

---

## Task 1: Cargo workspace scaffold

**Files:**
- Create: `Cargo.toml` (workspace)
- Create: `crates/gnome-clips-daemon/Cargo.toml`
- Create: `crates/gnome-clips-daemon/src/main.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
members = [
    "crates/gnome-clips-daemon",
]
resolver = "2"
```

- [ ] **Step 2: Create daemon crate**

```
mkdir -p crates/gnome-clips-daemon/src
```

- [ ] **Step 3: Create crates/gnome-clips-daemon/Cargo.toml**

```toml
[package]
name = "gnome-clips-daemon"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "gnome-clips-daemon"
path = "src/main.rs"

[dependencies]
tokio       = { version = "1",    features = ["full"] }
zbus        = { version = "4",    features = ["tokio"] }
rusqlite    = { version = "0.31", features = ["bundled"] }
serde       = { version = "1",    features = ["derive"] }
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror   = "1"
dirs        = "5"
sha2        = "0.10"
wl-clipboard-rs = "0.8"
x11-clipboard   = "0.9"

[dev-dependencies]
tempfile    = "3"
```

- [ ] **Step 4: Create stub main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo build -p gnome-clips-daemon
```

Expected: compiles with no errors (dependency download on first run).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/
git commit -m "chore: scaffold cargo workspace and gnome-clips-daemon crate"
```

---

## Task 2: Error types

**Files:**
- Create: `crates/gnome-clips-daemon/src/error.rs`
- Modify: `crates/gnome-clips-daemon/src/main.rs`

- [ ] **Step 1: Write the test**

```rust
// crates/gnome-clips-daemon/src/error.rs  (add at bottom after impl)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_error_displays() {
        let e = Error::Db(rusqlite::Error::InvalidColumnName("x".into()));
        assert!(e.to_string().contains("database"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p gnome-clips-daemon
```

Expected: FAIL — `error` module not found.

- [ ] **Step 3: Implement error.rs**

```rust
// crates/gnome-clips-daemon/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("clipboard error: {0}")]
    Clipboard(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 4: Declare module in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod error;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test -p gnome-clips-daemon error
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-clips-daemon/src/
git commit -m "feat(daemon): add unified error types"
```

---

## Task 3: Database initialisation and schema

**Files:**
- Create: `crates/gnome-clips-daemon/src/db/schema.rs`
- Create: `crates/gnome-clips-daemon/src/db/mod.rs`
- Modify: `crates/gnome-clips-daemon/src/main.rs`

- [ ] **Step 1: Write the test**

```rust
// At the bottom of crates/gnome-clips-daemon/src/db/mod.rs (add after struct)
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p gnome-clips-daemon db
```

Expected: FAIL — `db` module not found.

- [ ] **Step 3: Create schema.rs**

```rust
// crates/gnome-clips-daemon/src/db/schema.rs
pub const CREATE_CLIPS: &str = "
    CREATE TABLE IF NOT EXISTS clips (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        content      BLOB    NOT NULL,
        content_type TEXT    NOT NULL,
        preview      TEXT,
        source_app   TEXT,
        created_at   INTEGER NOT NULL,
        pinned       INTEGER NOT NULL DEFAULT 0,
        deleted      INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at);
    CREATE INDEX IF NOT EXISTS idx_clips_pinned     ON clips(pinned);
";

pub const CREATE_TAGS: &str = "
    CREATE TABLE IF NOT EXISTS tags (
        id   INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE
    );
";

pub const CREATE_CLIP_TAGS: &str = "
    CREATE TABLE IF NOT EXISTS clip_tags (
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
        PRIMARY KEY (clip_id, tag_id)
    );
";

pub const CREATE_SETTINGS: &str = "
    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";

pub const CREATE_EXCLUSIONS: &str = "
    CREATE TABLE IF NOT EXISTS exclusions (
        app_id TEXT PRIMARY KEY
    );
";
```

- [ ] **Step 4: Create db/mod.rs**

```rust
// crates/gnome-clips-daemon/src/db/mod.rs
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
```

- [ ] **Step 5: Create stub submodules so it compiles**

```rust
// crates/gnome-clips-daemon/src/db/clips.rs
// (empty stub — implemented in Task 4)
```

```rust
// crates/gnome-clips-daemon/src/db/tags.rs
// (empty stub — implemented in Task 5)
```

```rust
// crates/gnome-clips-daemon/src/db/settings.rs
// (empty stub — implemented in Task 6)
```

```rust
// crates/gnome-clips-daemon/src/db/exclusions.rs
// (empty stub — implemented in Task 7)
```

- [ ] **Step 6: Declare db module in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod db;
mod error;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 7: Run test to verify it passes**

```bash
cargo test -p gnome-clips-daemon db::tests
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/gnome-clips-daemon/src/
git commit -m "feat(daemon): database initialisation and schema migration"
```

---

## Task 4: Clips CRUD

**Files:**
- Modify: `crates/gnome-clips-daemon/src/db/clips.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/db/clips.rs  (add at bottom)
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon db::clips
```

Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement clips.rs**

```rust
// crates/gnome-clips-daemon/src/db/clips.rs
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

    // Build query dynamically to avoid N separate prepared statements
    let mut sql = "SELECT id, content_type, preview, source_app, created_at, pinned
                   FROM clips WHERE deleted = 0".to_string();
    if pinned_only { sql.push_str(" AND pinned = 1"); }
    if let Some(ref t) = type_filter {
        // support "image/*" prefix matching
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

    // Bind parameters positionally
    let mut idx = 1usize;
    let type_bind: Option<String> = type_filter.map(|t| {
        if t.ends_with("/*") {
            format!("{}%", &t[..t.len() - 1])
        } else {
            t.to_string()
        }
    });

    let rows = stmt.query_map(
        rusqlite::params_from_iter(
            type_bind.as_deref().map(rusqlite::types::Value::from)
                .into_iter()
                .chain(search_pat.as_deref().map(rusqlite::types::Value::from))
                .chain([
                    rusqlite::types::Value::Integer(limit as i64),
                    rusqlite::types::Value::Integer(offset as i64),
                ])
        ),
        |row| {
            Ok(ClipRow {
                id: row.get(0)?,
                content_type: row.get(1)?,
                preview: row.get(2)?,
                source_app: row.get(3)?,
                created_at: row.get(4)?,
                pinned: row.get::<_, i64>(5)? != 0,
            })
        },
    )?
    .collect::<Result<Vec<_>>>()?;

    Ok(rows)
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon db::clips
```

Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-clips-daemon/src/db/clips.rs
git commit -m "feat(daemon): clips CRUD (insert, get, delete, set_pinned, get_history)"
```

---

## Task 5: Tags CRUD

**Files:**
- Modify: `crates/gnome-clips-daemon/src/db/tags.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/db/tags.rs  (add at bottom)
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
    fn remove_tag() {
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
        add_tag(&db, clip_id, "work").unwrap(); // should not error
        let tags = get_clip_tags(&db, clip_id).unwrap();
        assert_eq!(tags.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon db::tags
```

Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement tags.rs**

```rust
// crates/gnome-clips-daemon/src/db/tags.rs
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon db::tags
```

Expected: all 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-clips-daemon/src/db/tags.rs
git commit -m "feat(daemon): tags CRUD (add, remove, get per clip)"
```

---

## Task 6: Settings and Config

**Files:**
- Modify: `crates/gnome-clips-daemon/src/db/settings.rs`
- Create: `crates/gnome-clips-daemon/src/config.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/db/settings.rs  (add at bottom)
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon db::settings
```

Expected: FAIL.

- [ ] **Step 3: Implement settings.rs**

```rust
// crates/gnome-clips-daemon/src/db/settings.rs
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
```

- [ ] **Step 4: Implement config.rs**

```rust
// crates/gnome-clips-daemon/src/config.rs
use crate::db::{settings, Database};
use crate::error::Result;

pub struct Config {
    pub retention_days: u32,
    pub retention_count: u32,
    pub shortcut_key: String,
    pub incognito: bool,
}

impl Config {
    pub const DEFAULT_RETENTION_DAYS: u32 = 7;
    pub const DEFAULT_RETENTION_COUNT: u32 = 100;
    pub const DEFAULT_SHORTCUT: &'static str = "Super+V";

    pub fn load(db: &Database) -> Result<Self> {
        let retention_days = settings::get_setting(db, "retention_days")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::DEFAULT_RETENTION_DAYS);
        let retention_count = settings::get_setting(db, "retention_count")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::DEFAULT_RETENTION_COUNT);
        let shortcut_key = settings::get_setting(db, "shortcut_key")?
            .unwrap_or_else(|| Self::DEFAULT_SHORTCUT.to_string());
        let incognito = settings::get_setting(db, "incognito")?
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        Ok(Self { retention_days, retention_count, shortcut_key, incognito })
    }
}
```

- [ ] **Step 5: Declare config in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod config;
mod db;
mod error;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon db::settings
```

Expected: all 3 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/gnome-clips-daemon/src/
git commit -m "feat(daemon): settings CRUD and Config loader with defaults"
```

---

## Task 7: Exclusions

**Files:**
- Modify: `crates/gnome-clips-daemon/src/db/exclusions.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/db/exclusions.rs  (add at bottom)
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon db::exclusions
```

Expected: FAIL.

- [ ] **Step 3: Implement exclusions.rs**

```rust
// crates/gnome-clips-daemon/src/db/exclusions.rs
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon db::exclusions
```

Expected: all 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-clips-daemon/src/db/exclusions.rs
git commit -m "feat(daemon): exclusion list with password manager defaults"
```

---

## Task 8: Preview generation

**Files:**
- Create: `crates/gnome-clips-daemon/src/preview.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/preview.rs  (add at bottom)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preview_truncates_at_200_chars() {
        let long: String = "a".repeat(300);
        let preview = generate_preview(long.as_bytes(), "text/plain");
        assert_eq!(preview.len(), 200);
    }

    #[test]
    fn text_preview_returns_full_short_text() {
        let preview = generate_preview(b"hello world", "text/plain");
        assert_eq!(preview, "hello world");
    }

    #[test]
    fn html_preview_strips_tags() {
        let html = b"<h1>Hello</h1><p>World</p>";
        let preview = generate_preview(html, "text/html");
        assert!(preview.contains("Hello"));
        assert!(preview.contains("World"));
        assert!(!preview.contains('<'));
    }

    #[test]
    fn image_preview_shows_placeholder() {
        let preview = generate_preview(b"fake png data", "image/png");
        assert_eq!(preview, "[Image]");
    }

    #[test]
    fn file_preview_shows_placeholder() {
        let preview = generate_preview(b"/home/user/report.pdf", "application/file");
        assert_eq!(preview, "[File: report.pdf]");
    }

    #[test]
    fn markdown_preview_strips_syntax() {
        let md = b"# Title\n\n**bold** text";
        let preview = generate_preview(md, "text/markdown");
        assert!(preview.contains("Title"));
        assert!(preview.contains("bold"));
        assert!(!preview.contains('#'));
        assert!(!preview.contains("**"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon preview
```

Expected: FAIL — `preview` module not found.

- [ ] **Step 3: Implement preview.rs**

```rust
// crates/gnome-clips-daemon/src/preview.rs
const MAX_PREVIEW_LEN: usize = 200;

pub fn generate_preview(content: &[u8], content_type: &str) -> String {
    match content_type {
        "text/plain" => truncate_utf8(content, MAX_PREVIEW_LEN),
        "text/html" => {
            let raw = String::from_utf8_lossy(content);
            let stripped = strip_html_tags(&raw);
            truncate_str(&stripped, MAX_PREVIEW_LEN)
        }
        "text/markdown" => {
            let raw = String::from_utf8_lossy(content);
            let stripped = strip_markdown(&raw);
            truncate_str(&stripped, MAX_PREVIEW_LEN)
        }
        t if t.starts_with("image/") => "[Image]".to_string(),
        "application/file" => {
            let path = String::from_utf8_lossy(content);
            let filename = std::path::Path::new(path.trim())
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string());
            format!("[File: {}]", filename)
        }
        _ => truncate_utf8(content, MAX_PREVIEW_LEN),
    }
}

fn truncate_utf8(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    truncate_str(&s, max)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Minimal HTML tag stripper — removes anything inside < >.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Minimal Markdown syntax stripper — removes #, **, *, `, >.
fn strip_markdown(md: &str) -> String {
    md.lines()
        .map(|line| {
            let l = line.trim_start_matches('#').trim();
            let l = l.replace("**", "").replace('*', "").replace('`', "").replace('>', "");
            l
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
```

- [ ] **Step 4: Declare in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod config;
mod db;
mod error;
mod preview;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon preview
```

Expected: all 6 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-clips-daemon/src/preview.rs crates/gnome-clips-daemon/src/main.rs
git commit -m "feat(daemon): preview generation for all content types"
```

---

## Task 9: Retention job

**Files:**
- Create: `crates/gnome-clips-daemon/src/retention.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/retention.rs  (add at bottom)
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
        // Insert 105 clips
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
        // 100 unpinned (trimmed) + 5 pinned = 105 total? No — count limit is 100 non-pinned
        let pinned: Vec<_> = remaining.iter().filter(|c| c.pinned).collect();
        assert_eq!(pinned.len(), 5);
        let unpinned: Vec<_> = remaining.iter().filter(|c| !c.pinned).collect();
        assert_eq!(unpinned.len(), 100);
    }

    #[test]
    fn age_limit_deletes_old_clips() {
        let db = db();
        // Insert a clip with created_at 10 days ago
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64) - (10 * 86400);
        db.conn.execute(
            "INSERT INTO clips (content, content_type, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![b"old clip".to_vec(), "text/plain", old_ts],
        ).unwrap();
        // Insert a recent clip
        insert_clip(&db, b"new clip", "text/plain", None, None).unwrap();
        let cfg = Config { retention_days: 7, ..default_config() };
        run_retention(&db, &cfg).unwrap();
        let remaining = get_history(&db, "", "", 0, 100).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].preview.as_deref(), None); // new clip has no preview
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon retention
```

Expected: FAIL.

- [ ] **Step 3: Implement retention.rs**

```rust
// crates/gnome-clips-daemon/src/retention.rs
use rusqlite::{params, Result};
use crate::config::Config;
use crate::db::Database;

pub fn run_retention(db: &Database, config: &Config) -> Result<()> {
    // 1. Delete clips older than retention_days (excluding pinned)
    let cutoff = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64)
        - (config.retention_days as i64 * 86400);

    db.conn.execute(
        "DELETE FROM clips WHERE pinned = 0 AND created_at < ?1",
        params![cutoff],
    )?;

    // 2. Trim to retention_count (keep the newest, excluding pinned from the count)
    //    Delete unpinned clips beyond the limit, ordered oldest-first.
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
```

- [ ] **Step 4: Declare in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod config;
mod db;
mod error;
mod preview;
mod retention;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon retention
```

Expected: all 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-clips-daemon/src/retention.rs crates/gnome-clips-daemon/src/main.rs
git commit -m "feat(daemon): retention job (age + count limits, pinned items exempt)"
```

---

## Task 10: D-Bus types

**Files:**
- Create: `crates/gnome-clips-daemon/src/dbus/mod.rs`
- Create: `crates/gnome-clips-daemon/src/dbus/types.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/dbus/types.rs  (add at bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{from_slice, to_bytes, EncodingContext, BE};

    fn ctx() -> EncodingContext<BE> {
        EncodingContext::<BE>::new_dbus(0)
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
        let decoded: ClipSummary = from_slice(&encoded, ctx()).unwrap();
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
        let decoded: ClipDetail = from_slice(&encoded, ctx()).unwrap();
        assert_eq!(decoded.content, vec![0x89, 0x50, 0x4e, 0x47]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon dbus::types
```

Expected: FAIL.

- [ ] **Step 3: Create dbus/mod.rs**

```rust
// crates/gnome-clips-daemon/src/dbus/mod.rs
pub mod types;
pub mod interface;
```

- [ ] **Step 4: Create dbus/types.rs**

```rust
// crates/gnome-clips-daemon/src/dbus/types.rs
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

/// Create a stub interface.rs so the module compiles.
// (Implemented in Task 11)
```

- [ ] **Step 5: Create stub interface.rs**

```rust
// crates/gnome-clips-daemon/src/dbus/interface.rs
// Implemented in Task 11
```

- [ ] **Step 6: Declare dbus module in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod config;
mod db;
mod dbus;
mod error;
mod preview;
mod retention;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 7: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon dbus::types
```

Expected: both PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/gnome-clips-daemon/src/dbus/
git commit -m "feat(daemon): D-Bus serialisable ClipSummary and ClipDetail types"
```

---

## Task 11: D-Bus interface — core methods

**Files:**
- Modify: `crates/gnome-clips-daemon/src/dbus/interface.rs`

- [ ] **Step 1: Write the tests**

```rust
// crates/gnome-clips-daemon/src/dbus/interface.rs  (add at bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{clips::insert_clip, Database};
    use std::sync::{Arc, Mutex};
    use tempfile::NamedTempFile;
    use tokio::sync::watch;

    async fn test_iface() -> ClipsInterface {
        let f = NamedTempFile::new().unwrap();
        let db = Database::open(f.path()).unwrap();
        insert_clip(&db, b"hello", "text/plain", Some("hello"), Some("gedit")).unwrap();
        let (_tx, rx) = watch::channel(false);
        ClipsInterface {
            db: Arc::new(Mutex::new(db)),
            incognito: rx,
        }
    }

    #[tokio::test]
    async fn get_history_returns_clips() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].content_type, "text/plain");
    }

    #[tokio::test]
    async fn get_clip_returns_content() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        let detail = iface.get_clip(id).await.unwrap();
        assert_eq!(detail.content, b"hello");
    }

    #[tokio::test]
    async fn delete_clip_removes_it() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        iface.delete_clip(id).await.unwrap();
        let clips_after = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert_eq!(clips_after.len(), 0);
    }

    #[tokio::test]
    async fn set_pinned_toggles_flag() {
        let iface = test_iface().await;
        let clips = iface.get_history("".to_string(), "".to_string(), 0, 1).await.unwrap();
        let id = clips[0].id;
        iface.set_pinned(id, true).await.unwrap();
        let clips_after = iface.get_history("".to_string(), "".to_string(), 0, 100).await.unwrap();
        assert!(clips_after[0].pinned);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon dbus::interface
```

Expected: FAIL.

- [ ] **Step 3: Implement interface.rs**

```rust
// crates/gnome-clips-daemon/src/dbus/interface.rs
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use zbus::interface;

use crate::db::{
    clips::{delete_clip, get_clip, get_history, insert_clip, set_pinned},
    exclusions::{add_exclusion, remove_exclusion},
    settings::{get_all_settings, set_setting},
    tags::{add_tag, get_clip_tags, remove_tag},
    Database,
};
use crate::dbus::types::{ClipDetail, ClipSummary};
use crate::preview::generate_preview;

pub struct ClipsInterface {
    pub db: Arc<Mutex<Database>>,
    /// Receives incognito state. `true` = incognito on.
    pub incognito: watch::Receiver<bool>,
}

fn map_err(e: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(e.to_string())
}

fn to_summary(db: &Database, row: &crate::db::clips::ClipRow) -> zbus::fdo::Result<ClipSummary> {
    let tags = get_clip_tags(db, row.id).map_err(map_err)?;
    Ok(ClipSummary {
        id: row.id,
        content_type: row.content_type.clone(),
        preview: row.preview.clone().unwrap_or_default(),
        source_app: row.source_app.clone().unwrap_or_default(),
        created_at: row.created_at,
        pinned: row.pinned,
        tags,
    })
}

#[interface(name = "org.gnome.Clips")]
impl ClipsInterface {
    async fn get_history(
        &self,
        filter: String,
        search: String,
        offset: u32,
        limit: u32,
    ) -> zbus::fdo::Result<Vec<ClipSummary>> {
        let db = self.db.lock().unwrap();
        let rows = get_history(&db, &filter, &search, offset, limit).map_err(map_err)?;
        rows.iter().map(|r| to_summary(&db, r)).collect()
    }

    async fn get_clip(&self, id: i64) -> zbus::fdo::Result<ClipDetail> {
        let db = self.db.lock().unwrap();
        let clip = get_clip(&db, id)
            .map_err(map_err)?
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("clip {} not found", id)))?;
        let tags = get_clip_tags(&db, id).map_err(map_err)?;
        Ok(ClipDetail {
            id: clip.id,
            content_type: clip.content_type,
            preview: clip.preview.unwrap_or_default(),
            source_app: clip.source_app.unwrap_or_default(),
            created_at: clip.created_at,
            pinned: clip.pinned,
            tags,
            content: clip.content,
        })
    }

    async fn delete_clip(&self, id: i64) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        delete_clip(&db, id).map_err(map_err)
    }

    async fn set_pinned(&self, id: i64, pinned: bool) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        set_pinned(&db, id, pinned).map_err(map_err)
    }

    async fn add_tag(&self, id: i64, tag: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        add_tag(&db, id, &tag).map_err(map_err)
    }

    async fn remove_tag(&self, id: i64, tag: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        remove_tag(&db, id, &tag).map_err(map_err)
    }

    async fn get_settings(&self) -> zbus::fdo::Result<std::collections::HashMap<String, String>> {
        let db = self.db.lock().unwrap();
        get_all_settings(&db).map_err(map_err)
    }

    async fn set_setting(&self, key: String, value: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        set_setting(&db, &key, &value).map_err(map_err)
    }

    async fn add_exclusion(&self, app_id: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        add_exclusion(&db, &app_id).map_err(map_err)
    }

    async fn remove_exclusion(&self, app_id: String) -> zbus::fdo::Result<()> {
        let db = self.db.lock().unwrap();
        remove_exclusion(&db, &app_id).map_err(map_err)
    }

    #[zbus(property)]
    async fn is_incognito(&self) -> bool {
        *self.incognito.borrow()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon dbus::interface
```

Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-clips-daemon/src/dbus/
git commit -m "feat(daemon): D-Bus interface with all CRUD and settings methods"
```

---

## Task 12: D-Bus signals

**Files:**
- Modify: `crates/gnome-clips-daemon/src/dbus/interface.rs`
- Create: `crates/gnome-clips-daemon/src/dbus/mod.rs` (replace stub)

The signals (`ClipAdded`, `ClipDeleted`, `ClipUpdated`, `IncognitoChanged`) are emitted by the daemon in response to clipboard events and D-Bus method calls. They are defined as `#[zbus(signal)]` methods on the interface. Since signals require a live `SignalContext`, they are emitted from `run_service()` in `dbus/mod.rs`, not from the interface impl itself.

- [ ] **Step 1: Add signal definitions to interface.rs**

Add the following inside `impl ClipsInterface` (below `remove_exclusion`):

```rust
    #[zbus(signal)]
    pub async fn clip_added(ctx: &zbus::SignalContext<'_>, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn clip_deleted(ctx: &zbus::SignalContext<'_>, id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn clip_updated(ctx: &zbus::SignalContext<'_>, clip: ClipSummary) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn incognito_changed(ctx: &zbus::SignalContext<'_>, enabled: bool) -> zbus::Result<()>;
```

- [ ] **Step 2: Implement run_service in dbus/mod.rs**

```rust
// crates/gnome-clips-daemon/src/dbus/mod.rs
pub mod interface;
pub mod types;

use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use zbus::ConnectionBuilder;

use crate::db::Database;
use crate::error::Result;
use interface::ClipsInterface;

pub enum DaemonEvent {
    ClipAdded(types::ClipSummary),
    ClipDeleted(i64),
    ClipUpdated(types::ClipSummary),
    IncognitoChanged(bool),
}

pub async fn run_service(
    db: Arc<Mutex<Database>>,
    incognito_rx: watch::Receiver<bool>,
    mut events: mpsc::Receiver<DaemonEvent>,
) -> Result<()> {
    let iface = ClipsInterface {
        db: db.clone(),
        incognito: incognito_rx,
    };

    let conn = ConnectionBuilder::session()?
        .name("org.gnome.Clips")?
        .serve_at("/org/gnome/Clips", iface)?
        .build()
        .await?;

    let object_server = conn.object_server();

    loop {
        let Some(event) = events.recv().await else { break };

        let iface_ref = object_server
            .interface::<_, ClipsInterface>("/org/gnome/Clips")
            .await?;
        let ctx = iface_ref.signal_context();

        match event {
            DaemonEvent::ClipAdded(clip) => {
                ClipsInterface::clip_added(ctx, clip).await?;
            }
            DaemonEvent::ClipDeleted(id) => {
                ClipsInterface::clip_deleted(ctx, id).await?;
            }
            DaemonEvent::ClipUpdated(clip) => {
                ClipsInterface::clip_updated(ctx, clip).await?;
            }
            DaemonEvent::IncognitoChanged(enabled) => {
                ClipsInterface::incognito_changed(ctx, enabled).await?;
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build -p gnome-clips-daemon
```

Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-clips-daemon/src/dbus/
git commit -m "feat(daemon): D-Bus signals and run_service event loop"
```

---

## Task 13: Clipboard monitor — Wayland

**Files:**
- Create: `crates/gnome-clips-daemon/src/clipboard/mod.rs`
- Create: `crates/gnome-clips-daemon/src/clipboard/wayland.rs`
- Create: `crates/gnome-clips-daemon/src/clipboard/x11.rs`

The monitor polls the clipboard every 500ms. It computes a SHA-256 hash of the content to detect changes without comparing large blobs.

- [ ] **Step 1: Write the test**

```rust
// crates/gnome-clips-daemon/src/clipboard/mod.rs  (add at bottom)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_differs_for_different_content() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_same_for_identical_content() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"hello");
        assert_eq!(h1, h2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon clipboard
```

Expected: FAIL.

- [ ] **Step 3: Create clipboard/mod.rs**

```rust
// crates/gnome-clips-daemon/src/clipboard/mod.rs
pub mod wayland;
pub mod x11;

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use std::time::Duration;

#[derive(Debug)]
pub struct ClipboardEvent {
    pub content: Vec<u8>,
    pub content_type: String,
    pub source_app: Option<String>,
}

pub type ContentHash = [u8; 32];

pub fn content_hash(data: &[u8]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Launches the appropriate clipboard monitor for the current session.
/// Sends events on `tx` whenever new clipboard content is detected.
pub async fn start_monitor(tx: mpsc::Sender<ClipboardEvent>) {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        tokio::spawn(wayland::poll_wayland(tx));
    } else {
        tokio::spawn(x11::poll_x11(tx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_differs_for_different_content() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_same_for_identical_content() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"hello");
        assert_eq!(h1, h2);
    }
}
```

- [ ] **Step 4: Create clipboard/wayland.rs**

```rust
// crates/gnome-clips-daemon/src/clipboard/wayland.rs
//
// Polls the Wayland clipboard every 500ms using wl-clipboard-rs.
// Detects changes by hashing content. Supports multiple MIME types,
// preferring richer types when available.

use std::io::Read;
use std::time::Duration;
use tokio::sync::mpsc;
use wl_clipboard_rs::paste::{get_contents, get_mime_types, ClipboardType, Error, MimeType, Seat};

use super::{content_hash, ClipboardEvent, ContentHash};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// MIME type priority — highest priority first.
const MIME_PRIORITY: &[&str] = &[
    "text/html",
    "text/markdown",
    "image/png",
    "image/jpeg",
    "application/octet-stream", // file
    "text/plain",
];

pub async fn poll_wayland(tx: mpsc::Sender<ClipboardEvent>) {
    let mut last_hash: Option<ContentHash> = None;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        if let Some(event) = read_clipboard() {
            let hash = content_hash(&event.content);
            if Some(hash) != last_hash {
                last_hash = Some(hash);
                let _ = tx.send(event).await;
            }
        }
    }
}

fn read_clipboard() -> Option<ClipboardEvent> {
    // Get available MIME types
    let types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(t) => t,
        Err(_) => return None,
    };

    // Pick the highest-priority available type
    let chosen_mime = MIME_PRIORITY
        .iter()
        .find(|&&m| types.contains(m))
        .copied()
        .or_else(|| {
            // Fall back to any text/* type
            types.iter().find(|t| t.starts_with("text/")).map(|s| s.as_str())
        })?;

    let result = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(chosen_mime),
    );

    match result {
        Ok((mut reader, _)) => {
            let mut content = Vec::new();
            if reader.read_to_end(&mut content).is_err() || content.is_empty() {
                return None;
            }
            // Map file MIME to our canonical type
            let content_type = if chosen_mime == "application/octet-stream" {
                "application/file".to_string()
            } else {
                chosen_mime.to_string()
            };
            Some(ClipboardEvent {
                content,
                content_type,
                source_app: None, // Wayland does not expose source app
            })
        }
        Err(Error::NoSeats) | Err(Error::ClipboardEmpty) | Err(Error::NoMimeType) => None,
        Err(_) => None,
    }
}
```

- [ ] **Step 5: Create clipboard/x11.rs**

```rust
// crates/gnome-clips-daemon/src/clipboard/x11.rs
//
// X11 clipboard monitor using x11-clipboard crate.
// Polls every 500ms as Wayland monitor does.

use std::time::Duration;
use tokio::sync::mpsc;
use x11_clipboard::Clipboard;

use super::{content_hash, ClipboardEvent, ContentHash};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub async fn poll_x11(tx: mpsc::Sender<ClipboardEvent>) {
    let clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => {
            tracing::error!("failed to open X11 clipboard");
            return;
        }
    };

    let mut last_hash: Option<ContentHash> = None;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let content = clipboard.load(
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            Duration::from_millis(100),
        );

        if let Ok(content) = content {
            if content.is_empty() {
                continue;
            }
            let hash = content_hash(&content);
            if Some(hash) != last_hash {
                last_hash = Some(hash);
                let _ = tx
                    .send(ClipboardEvent {
                        content,
                        content_type: "text/plain".to_string(),
                        source_app: None,
                    })
                    .await;
            }
        }
    }
}
```

- [ ] **Step 6: Declare clipboard module in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod clipboard;
mod config;
mod db;
mod dbus;
mod error;
mod preview;
mod retention;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 7: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon clipboard
```

Expected: both hash tests PASS.

- [ ] **Step 8: Verify it compiles**

```bash
cargo build -p gnome-clips-daemon
```

Expected: builds cleanly.

- [ ] **Step 9: Commit**

```bash
git add crates/gnome-clips-daemon/src/clipboard/ crates/gnome-clips-daemon/src/main.rs
git commit -m "feat(daemon): clipboard monitor (Wayland polling + X11 fallback)"
```

---

## Task 14: Incognito mode

**Files:**
- Create: `crates/gnome-clips-daemon/src/incognito.rs`

Incognito state is a `tokio::sync::watch` channel: the clipboard consumer checks it before persisting, and the D-Bus interface exposes it as both a property and via `SetIncognito`.

- [ ] **Step 1: Write the test**

```rust
// crates/gnome-clips-daemon/src/incognito.rs  (add at bottom)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_changes_state() {
        let state = IncognitoState::new(false);
        assert!(!state.get());
        state.set(true);
        assert!(state.get());
        state.set(false);
        assert!(!state.get());
    }

    #[test]
    fn receiver_sees_update() {
        let state = IncognitoState::new(false);
        let mut rx = state.subscribe();
        state.set(true);
        // mark_changed so receiver sees it
        assert!(*rx.borrow_and_update());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p gnome-clips-daemon incognito
```

Expected: FAIL.

- [ ] **Step 3: Implement incognito.rs**

```rust
// crates/gnome-clips-daemon/src/incognito.rs
use tokio::sync::watch;

/// Shared incognito state. Wraps a `watch` channel so multiple consumers
/// (clipboard loop, D-Bus interface) can read and react to changes.
pub struct IncognitoState {
    tx: watch::Sender<bool>,
}

impl IncognitoState {
    pub fn new(initial: bool) -> Self {
        let (tx, _) = watch::channel(initial);
        Self { tx }
    }

    pub fn set(&self, enabled: bool) {
        let _ = self.tx.send(enabled);
    }

    pub fn get(&self) -> bool {
        *self.tx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 4: Wire SetIncognito into the D-Bus interface**

In `interface.rs`, add a `set_incognito` method that sends on a `watch::Sender<bool>` (passed in alongside the receiver). Update `ClipsInterface` to hold a `watch::Sender<bool>` as `incognito_tx`:

```rust
// Add to ClipsInterface struct:
pub incognito_tx: watch::Sender<bool>,

// Add method inside #[interface] impl:
async fn set_incognito(&self, enabled: bool) -> zbus::fdo::Result<()> {
    let _ = self.incognito_tx.send(enabled);
    Ok(())
}
```

Update `run_service` in `dbus/mod.rs` to accept `incognito_tx: watch::Sender<bool>` and pass it to `ClipsInterface`.

- [ ] **Step 5: Declare in main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod clipboard;
mod config;
mod db;
mod dbus;
mod error;
mod incognito;
mod preview;
mod retention;

fn main() {
    println!("gnome-clips-daemon stub");
}
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cargo test -p gnome-clips-daemon incognito
```

Expected: both tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/gnome-clips-daemon/src/
git commit -m "feat(daemon): incognito mode via watch channel"
```

---

## Task 15: Wire up main.rs and systemd service

**Files:**
- Modify: `crates/gnome-clips-daemon/src/main.rs`
- Create: `dist/systemd/gnome-clips-daemon.service`

This task wires all subsystems together: opens the DB, seeds exclusions, loads config, starts the clipboard monitor, starts the hourly retention job, and starts the D-Bus service.

- [ ] **Step 1: Implement main.rs**

```rust
// crates/gnome-clips-daemon/src/main.rs
mod clipboard;
mod config;
mod db;
mod dbus;
mod error;
mod incognito;
mod preview;
mod retention;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::db::{exclusions::seed_defaults, Database};
use crate::config::Config;
use crate::dbus::DaemonEvent;
use crate::incognito::IncognitoState;

#[tokio::main]
async fn main() -> error::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("gnome-clips-daemon starting");

    // Open database
    let database = Database::open_default()?;
    seed_defaults(&database)?;
    let config = Config::load(&database)?;
    let db = Arc::new(Mutex::new(database));

    // Incognito state
    let incognito = IncognitoState::new(config.incognito);
    let incognito_rx = incognito.subscribe();
    let (incognito_tx, _incognito_watch_rx) = watch::channel(config.incognito);

    // D-Bus event channel
    let (event_tx, event_rx) = mpsc::channel::<DaemonEvent>(64);

    // Clipboard monitor
    let (clip_tx, mut clip_rx) = mpsc::channel::<clipboard::ClipboardEvent>(64);
    clipboard::start_monitor(clip_tx).await;

    // Clipboard consumer task
    {
        let db = db.clone();
        let event_tx = event_tx.clone();
        let incognito_rx2 = incognito.subscribe();
        tokio::spawn(async move {
            while let Some(event) = clip_rx.recv().await {
                // Skip if incognito
                if *incognito_rx2.borrow() {
                    continue;
                }

                let (id, summary) = {
                    let db = db.lock().unwrap();

                    // Skip if source app is excluded
                    if let Some(ref app) = event.source_app {
                        if db::exclusions::is_excluded(&db, app).unwrap_or(false) {
                            continue;
                        }
                    }

                    let preview = preview::generate_preview(&event.content, &event.content_type);
                    let id = db::clips::insert_clip(
                        &db,
                        &event.content,
                        &event.content_type,
                        Some(&preview),
                        event.source_app.as_deref(),
                    )
                    .unwrap();

                    let tags = db::tags::get_clip_tags(&db, id).unwrap_or_default();
                    let clip = db::clips::get_clip(&db, id).unwrap().unwrap();

                    let summary = dbus::types::ClipSummary {
                        id,
                        content_type: clip.content_type,
                        preview: clip.preview.unwrap_or_default(),
                        source_app: clip.source_app.unwrap_or_default(),
                        created_at: clip.created_at,
                        pinned: clip.pinned,
                        tags,
                    };
                    (id, summary)
                };

                let _ = event_tx.send(DaemonEvent::ClipAdded(summary)).await;
                info!(clip_id = id, "clip stored");
            }
        });
    }

    // Hourly retention job
    {
        let db = db.clone();
        let retention_days = config.retention_days;
        let retention_count = config.retention_count;
        tokio::spawn(async move {
            let cfg = config::Config {
                retention_days,
                retention_count,
                shortcut_key: String::new(),
                incognito: false,
            };
            // Run once at startup, then every hour
            retention::run_retention(&db.lock().unwrap(), &cfg).ok();
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            loop {
                interval.tick().await;
                retention::run_retention(&db.lock().unwrap(), &cfg).ok();
            }
        });
    }

    // D-Bus service (runs until channel closes)
    dbus::run_service(db, incognito_rx, incognito_tx, event_rx).await?;

    Ok(())
}
```

- [ ] **Step 2: Update run_service signature in dbus/mod.rs**

Update `run_service` to accept `incognito_tx` and pass it to `ClipsInterface`:

```rust
pub async fn run_service(
    db: Arc<Mutex<Database>>,
    incognito_rx: watch::Receiver<bool>,
    incognito_tx: watch::Sender<bool>,
    mut events: mpsc::Receiver<DaemonEvent>,
) -> Result<()> {
    let iface = ClipsInterface {
        db: db.clone(),
        incognito: incognito_rx,
        incognito_tx,
    };
    // ... rest unchanged
```

- [ ] **Step 3: Create systemd service file**

```
mkdir -p dist/systemd
```

```ini
# dist/systemd/gnome-clips-daemon.service
[Unit]
Description=gnome-clips clipboard history daemon
Documentation=https://github.com/gnome-power-toys/gnome-clips
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/gnome-clips-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=graphical-session.target
```

- [ ] **Step 4: Build and verify**

```bash
cargo build -p gnome-clips-daemon --release
```

Expected: builds cleanly, produces `target/release/gnome-clips-daemon`.

- [ ] **Step 5: Smoke test (requires a running GNOME/Wayland session)**

```bash
RUST_LOG=info ./target/release/gnome-clips-daemon &
# In another terminal:
gdbus call --session \
    --dest org.gnome.Clips \
    --object-path /org/gnome/Clips \
    --method org.gnome.Clips.GetHistory \
    "" "" 0 10
# Expected: ([]  ,) — empty array, no errors
```

- [ ] **Step 6: Run full test suite**

```bash
cargo test -p gnome-clips-daemon
```

Expected: all tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/gnome-clips-daemon/src/main.rs dist/systemd/
git commit -m "feat(daemon): wire all subsystems in main.rs and add systemd unit"
```

---

## Self-Review Checklist

- **Spec §1 Architecture** — daemon crate ✓, systemd unit ✓, D-Bus at `org.gnome.Clips` ✓
- **Spec §2 Content types** — all six MIME types handled in `wayland.rs` MIME priority list and `preview.rs` ✓
- **Spec §3 Data model** — all five tables ✓, retention job ✓, hard-delete on user action ✓, preview generation ✓
- **Spec §4 D-Bus interface** — all 11 methods ✓, all 4 signals ✓, `ClipSummary`/`ClipDetail` types ✓, `filter` valid values documented ✓
- **Spec §7 Privacy** — exclusion list with default password managers ✓, incognito mode with watch channel ✓, no content logging ✓ (only clip_id logged)
- **Spec §9 Platform** — Wayland primary ✓, X11 fallback ✓, session detection via `$WAYLAND_DISPLAY` ✓
