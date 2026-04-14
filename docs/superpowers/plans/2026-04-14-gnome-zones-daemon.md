# gnome-zones Daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `gnome-zones-daemon`, a systemd user service that owns zone layouts, registers global keyboard shortcuts, and snaps the focused window into a zone on demand. Exposes everything over D-Bus at `org.gnome.Zones`.

**Architecture:** A tokio async service with four subsystems: (a) SQLite persistence (layouts, zones, per-monitor assignments, settings); (b) a `WindowMover` trait with two implementations — one using `org.gnome.Mutter.DisplayConfig`, one using a tiny custom GNOME Shell extension shim; (c) a zbus D-Bus service exposing layout CRUD and snap actions; (d) first-run hotkey registration through `org.gnome.settings-daemon.plugins.media-keys` that invokes the daemon via `busctl`.

**Tech Stack:** Rust 2021, tokio 1, zbus 4 (D-Bus), rusqlite 0.31 (bundled SQLite), tracing 0.1, thiserror 1, serde 1, sha2 0.10. The Shell-extension shim is JavaScript (GJS) — no build step.

**Spec:** `docs/superpowers/specs/2026-04-14-gnome-zones-design.md`

**This is Plan 1 of 3.** Plan 2 covers the GTK4 UI (`gnome-zones`). Plan 3 covers packaging (Flatpak + `.deb`).

**Scope of this plan:** Daemon + Shell-extension shim + hotkey wiring. After this plan executes, the user can press `Super+Ctrl+1`…`9` to snap the focused window to seeded preset zones. The activator overlay, zone editor, and panel icon are all UI — they come in Plan 2.

---

## File Structure

```
gnome-power-toys/
├── Cargo.toml                                        # workspace (create or extend)
├── crates/
│   └── gnome-zones-daemon/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                               # tokio entry, wires all subsystems
│           ├── error.rs                              # Error + Result types
│           ├── model.rs                              # ZoneRect, Layout, LayoutSummary, MonitorInfo, Direction
│           ├── math.rs                               # project_rect, deflate, bounding_rect, iterate_index
│           ├── db/
│           │   ├── mod.rs                            # Database, open, migrate
│           │   ├── schema.rs                         # SQL DDL strings
│           │   ├── layouts.rs                        # layout CRUD + zones replacement
│           │   ├── monitors.rs                       # monitor_assignments CRUD
│           │   └── settings.rs                       # key/value settings
│           ├── presets.rs                            # preset definitions + seed()
│           ├── monitors.rs                           # MonitorService — DisplayConfig enumeration + monitor_key
│           ├── window/
│           │   ├── mod.rs                            # WindowMover trait, focused_window_id()
│           │   ├── mutter.rs                         # MutterMover via Mutter D-Bus
│           │   └── shim.rs                           # ShimMover via our extension
│           ├── snap/
│           │   ├── mod.rs                            # SnapEngine — the action methods
│           │   └── state.rs                          # in-memory WindowStateMap
│           ├── hotkeys.rs                            # gsettings stash/restore/register
│           └── dbus/
│               ├── mod.rs                            # run_service
│               ├── types.rs                          # wire types for zbus
│               └── interface.rs                      # ZonesInterface
└── dist/
    └── shell-extension/
        └── gnome-zones-mover@power-toys/
            ├── metadata.json
            └── extension.js
```

---

## Conventions for this plan

- **All `cargo test` invocations are scoped:** `cargo test -p gnome-zones-daemon …`. Never `--workspace`.
- **TDD cadence for each task:** write the failing test → run → fail → implement → run → pass → commit.
- **Commits are frequent** (one per task).
- **Time format for DB:** Unix seconds (`i64`).
- **Coordinate conventions:** `(x, y)` is top-left, `(w, h)` is size. Fractional coordinates in `[0.0, 1.0]` unless explicitly pixel.

---

## Task 1: Cargo workspace scaffold

**Files:**
- Create or modify: `Cargo.toml` (workspace root)
- Create: `crates/gnome-zones-daemon/Cargo.toml`
- Create: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Ensure workspace Cargo.toml exists and lists our crate**

If `Cargo.toml` does not exist at the repo root, create it:

```toml
# Cargo.toml
[workspace]
members = [
    "crates/gnome-zones-daemon",
]
resolver = "2"
```

If it exists (e.g., because the gnome-clips plan was executed first), add `"crates/gnome-zones-daemon"` to the existing `members` array.

- [ ] **Step 2: Create the daemon crate directory**

```bash
mkdir -p crates/gnome-zones-daemon/src
```

- [ ] **Step 3: Create `crates/gnome-zones-daemon/Cargo.toml`**

```toml
[package]
name    = "gnome-zones-daemon"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "gnome-zones-daemon"
path = "src/main.rs"

[dependencies]
tokio              = { version = "1",    features = ["full"] }
zbus               = { version = "4",    features = ["tokio"] }
rusqlite           = { version = "0.31", features = ["bundled"] }
serde              = { version = "1",    features = ["derive"] }
tracing            = "0.1"
tracing-subscriber = { version = "0.3",  features = ["env-filter"] }
thiserror          = "1"
dirs               = "5"
sha2               = "0.10"
hex                = "0.4"
futures-util       = "0.3"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create stub `src/main.rs`**

```rust
// crates/gnome-zones-daemon/src/main.rs
fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo build -p gnome-zones-daemon
```

Expected: compiles after dependency download.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/gnome-zones-daemon
git commit -m "chore(zones): scaffold gnome-zones-daemon crate"
```

---

## Task 2: Error types

**Files:**
- Create: `crates/gnome-zones-daemon/src/error.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Create `src/error.rs` with tests**

```rust
// crates/gnome-zones-daemon/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no focused window")]
    NoFocusedWindow,

    #[error("no layout assigned to monitor {0}")]
    NoLayoutForMonitor(String),

    #[error("invalid zone index {0} (layout has {1} zones)")]
    InvalidZoneIndex(u32, u32),

    #[error("config error: {0}")]
    Config(String),

    #[error("compositor error: {0}")]
    Compositor(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_error_displays() {
        let e = Error::Db(rusqlite::Error::InvalidColumnName("x".into()));
        assert!(e.to_string().contains("database"));
    }

    #[test]
    fn invalid_zone_index_displays_both_numbers() {
        let e = Error::InvalidZoneIndex(7, 4);
        let msg = e.to_string();
        assert!(msg.contains('7') && msg.contains('4'));
    }
}
```

- [ ] **Step 2: Declare module in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod error;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon error
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): unified error types"
```

---

## Task 3: Model types

**Files:**
- Create: `crates/gnome-zones-daemon/src/model.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/gnome-zones-daemon/src/model.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoneRect {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl ZoneRect {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn is_normalized(&self) -> bool {
        self.x >= 0.0 && self.y >= 0.0
            && self.w > 0.0 && self.h > 0.0
            && self.x + self.w <= 1.0 + f64::EPSILON
            && self.y + self.h <= 1.0 + f64::EPSILON
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSummary {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneRect>,
}

impl Layout {
    pub fn zone(&self, zone_index: u32) -> Option<&ZoneRect> {
        self.zones.iter().find(|z| z.zone_index == zone_index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub monitor_key: String,
    pub connector: String,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterateDir {
    Prev,
    Next,
}

impl std::str::FromStr for IterateDir {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "prev" => Ok(Self::Prev),
            "next" => Ok(Self::Next),
            other  => Err(format!("unknown direction: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_center_is_midpoint() {
        let r = ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 0.5, h: 1.0 };
        assert_eq!(r.center(), (0.25, 0.5));
    }

    #[test]
    fn normalized_accepts_full_rect() {
        let full = ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        assert!(full.is_normalized());
    }

    #[test]
    fn normalized_rejects_negative_origin() {
        let bad = ZoneRect { zone_index: 1, x: -0.1, y: 0.0, w: 0.5, h: 0.5 };
        assert!(!bad.is_normalized());
    }

    #[test]
    fn normalized_rejects_overflow() {
        let bad = ZoneRect { zone_index: 1, x: 0.5, y: 0.0, w: 0.6, h: 1.0 };
        assert!(!bad.is_normalized());
    }

    #[test]
    fn layout_zone_lookup_by_index() {
        let layout = Layout {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 0.5, h: 1.0 },
                ZoneRect { zone_index: 2, x: 0.5, y: 0.0, w: 0.5, h: 1.0 },
            ],
        };
        assert_eq!(layout.zone(2).unwrap().x, 0.5);
        assert!(layout.zone(99).is_none());
    }

    #[test]
    fn iterate_dir_parses() {
        assert_eq!("prev".parse::<IterateDir>().unwrap(), IterateDir::Prev);
        assert_eq!("next".parse::<IterateDir>().unwrap(), IterateDir::Next);
        assert!("sideways".parse::<IterateDir>().is_err());
    }
}
```

- [ ] **Step 2: Declare module in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod error;
mod model;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon model
```

Expected: 6 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): model types (ZoneRect, Layout, MonitorInfo, IterateDir)"
```

---

## Task 4: Zone rect math — projection, deflation, bounding rect

**Files:**
- Create: `crates/gnome-zones-daemon/src/math.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Create `src/math.rs`**

```rust
// crates/gnome-zones-daemon/src/math.rs
use crate::model::ZoneRect;

/// Pixel rectangle. Same layout everywhere: top-left + size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Project a fractional zone onto a monitor of given pixel size.
pub fn project_rect(zone: &ZoneRect, monitor_w: i32, monitor_h: i32) -> PixelRect {
    PixelRect {
        x: (zone.x * monitor_w as f64).round() as i32,
        y: (zone.y * monitor_h as f64).round() as i32,
        w: (zone.w * monitor_w as f64).round() as i32,
        h: (zone.h * monitor_h as f64).round() as i32,
    }
}

/// Shrink all four sides by `gap` pixels. A rect smaller than `2*gap`
/// on either axis clamps to a single pixel so we never emit negative sizes.
pub fn deflate(rect: PixelRect, gap: i32) -> PixelRect {
    let w = (rect.w - 2 * gap).max(1);
    let h = (rect.h - 2 * gap).max(1);
    PixelRect { x: rect.x + gap, y: rect.y + gap, w, h }
}

/// Bounding rectangle (union) of the given zones, in fractional coords.
/// Panics if `zones` is empty — callers must ensure at least one zone.
pub fn bounding_rect(zones: &[&ZoneRect]) -> ZoneRect {
    assert!(!zones.is_empty(), "bounding_rect of empty set");
    let x0 = zones.iter().map(|z| z.x).fold(f64::INFINITY, f64::min);
    let y0 = zones.iter().map(|z| z.y).fold(f64::INFINITY, f64::min);
    let x1 = zones.iter().map(|z| z.x + z.w).fold(f64::NEG_INFINITY, f64::max);
    let y1 = zones.iter().map(|z| z.y + z.h).fold(f64::NEG_INFINITY, f64::max);
    ZoneRect {
        zone_index: zones[0].zone_index,  // caller picks one; irrelevant for layout
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn project_full_screen() {
        let r = project_rect(&z(1, 0.0, 0.0, 1.0, 1.0), 1920, 1080);
        assert_eq!(r, PixelRect { x: 0, y: 0, w: 1920, h: 1080 });
    }

    #[test]
    fn project_right_third() {
        let r = project_rect(&z(1, 2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0), 1920, 1080);
        assert!((r.x - 1280).abs() <= 1);
        assert!((r.w - 640).abs()  <= 1);
    }

    #[test]
    fn deflate_shrinks_four_sides() {
        let r = deflate(PixelRect { x: 0, y: 0, w: 100, h: 100 }, 8);
        assert_eq!(r, PixelRect { x: 8, y: 8, w: 84, h: 84 });
    }

    #[test]
    fn deflate_clamps_when_too_small() {
        let r = deflate(PixelRect { x: 0, y: 0, w: 4, h: 100 }, 8);
        assert_eq!(r.w, 1);
        assert_eq!(r.h, 84);
    }

    #[test]
    fn bounding_rect_of_two_columns() {
        let a = z(1, 0.0, 0.0, 0.5, 1.0);
        let b = z(2, 0.5, 0.0, 0.5, 1.0);
        let u = bounding_rect(&[&a, &b]);
        assert!((u.x - 0.0).abs() < 1e-9);
        assert!((u.w - 1.0).abs() < 1e-9);
        assert!((u.h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn bounding_rect_of_disjoint_zones() {
        let a = z(1, 0.0, 0.0, 0.3, 0.5);
        let b = z(2, 0.7, 0.5, 0.3, 0.5);
        let u = bounding_rect(&[&a, &b]);
        assert!((u.x - 0.0).abs() < 1e-9);
        assert!((u.y - 0.0).abs() < 1e-9);
        assert!((u.w - 1.0).abs() < 1e-9);
        assert!((u.h - 1.0).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Declare module**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod error;
mod math;
mod model;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon math
```

Expected: 6 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): zone-rect math (project/deflate/bounding_rect)"
```

---

## Task 5: Indexed iteration logic

**Files:**
- Modify: `crates/gnome-zones-daemon/src/math.rs`

Spec §4 "Indexed iteration": `Super+Right` → `(current % zone_count) + 1`; `Super+Left` → `((current - 2 + zone_count) % zone_count) + 1`. Unsnapped windows treat `current_index = 0` so the first `Right` press lands on zone 1 and the first `Left` press lands on the last zone.

- [ ] **Step 1: Add iteration logic + tests to `math.rs`**

Append to the bottom of `src/math.rs`:

```rust
use crate::model::IterateDir;

/// Compute the next zone index for indexed iteration.
///
/// `current_index` — 1-based current zone, or 0 if the window isn't snapped.
/// `zone_count`    — total zones on the active layout (must be > 0).
///
/// Returns the new 1-based index. Wraps.
pub fn iterate_index(current_index: u32, zone_count: u32, dir: IterateDir) -> u32 {
    assert!(zone_count > 0, "iterate_index needs at least one zone");
    match dir {
        IterateDir::Next => {
            // Unsnapped (0) → 1. Snapped → (current mod n) + 1.
            if current_index == 0 {
                1
            } else {
                (current_index % zone_count) + 1
            }
        }
        IterateDir::Prev => {
            // Unsnapped (0) → last. Snapped → ((c - 2 + n) mod n) + 1.
            if current_index == 0 {
                zone_count
            } else {
                ((current_index + zone_count - 2) % zone_count) + 1
            }
        }
    }
}
```

And append these tests to the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn iterate_next_wraps() {
        assert_eq!(iterate_index(1, 3, IterateDir::Next), 2);
        assert_eq!(iterate_index(2, 3, IterateDir::Next), 3);
        assert_eq!(iterate_index(3, 3, IterateDir::Next), 1);
    }

    #[test]
    fn iterate_prev_wraps() {
        assert_eq!(iterate_index(3, 3, IterateDir::Prev), 2);
        assert_eq!(iterate_index(2, 3, IterateDir::Prev), 1);
        assert_eq!(iterate_index(1, 3, IterateDir::Prev), 3);
    }

    #[test]
    fn iterate_unsnapped_next_lands_on_1() {
        assert_eq!(iterate_index(0, 5, IterateDir::Next), 1);
    }

    #[test]
    fn iterate_unsnapped_prev_lands_on_last() {
        assert_eq!(iterate_index(0, 5, IterateDir::Prev), 5);
    }

    #[test]
    fn iterate_single_zone_is_fixpoint() {
        assert_eq!(iterate_index(1, 1, IterateDir::Next), 1);
        assert_eq!(iterate_index(1, 1, IterateDir::Prev), 1);
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p gnome-zones-daemon math::tests::iterate
```

Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/math.rs
git commit -m "feat(zones-daemon): indexed iteration math"
```

---

## Task 6: Database init + schema

**Files:**
- Create: `crates/gnome-zones-daemon/src/db/mod.rs`
- Create: `crates/gnome-zones-daemon/src/db/schema.rs`
- Create: `crates/gnome-zones-daemon/src/db/layouts.rs` (empty stub)
- Create: `crates/gnome-zones-daemon/src/db/monitors.rs` (empty stub)
- Create: `crates/gnome-zones-daemon/src/db/settings.rs` (empty stub)
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Create `src/db/schema.rs`**

```rust
// crates/gnome-zones-daemon/src/db/schema.rs
pub const CREATE_LAYOUTS: &str = "
    CREATE TABLE IF NOT EXISTS layouts (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        name        TEXT    NOT NULL,
        is_preset   INTEGER NOT NULL DEFAULT 0,
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_layouts_is_preset ON layouts(is_preset);
";

pub const CREATE_ZONES: &str = "
    CREATE TABLE IF NOT EXISTS zones (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        layout_id  INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE,
        zone_index INTEGER NOT NULL,
        x          REAL    NOT NULL,
        y          REAL    NOT NULL,
        w          REAL    NOT NULL,
        h          REAL    NOT NULL,
        UNIQUE (layout_id, zone_index)
    );
    CREATE INDEX IF NOT EXISTS idx_zones_layout ON zones(layout_id);
";

pub const CREATE_MONITOR_ASSIGNMENTS: &str = "
    CREATE TABLE IF NOT EXISTS monitor_assignments (
        monitor_key TEXT    PRIMARY KEY,
        layout_id   INTEGER NOT NULL REFERENCES layouts(id),
        updated_at  INTEGER NOT NULL
    );
";

pub const CREATE_SETTINGS: &str = "
    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";
```

- [ ] **Step 2: Create `src/db/mod.rs`**

```rust
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
```

- [ ] **Step 3: Create empty stubs so it compiles**

```rust
// crates/gnome-zones-daemon/src/db/layouts.rs
// implemented in Task 7
```

```rust
// crates/gnome-zones-daemon/src/db/monitors.rs
// implemented in Task 8
```

```rust
// crates/gnome-zones-daemon/src/db/settings.rs
// implemented in Task 9
```

- [ ] **Step 4: Declare modules in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod error;
mod math;
mod model;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p gnome-zones-daemon db
```

Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): SQLite schema + migrations"
```

---

## Task 7: Layout CRUD

**Files:**
- Modify: `crates/gnome-zones-daemon/src/db/layouts.rs`

All layout mutations replace the full zone list atomically — we never mutate individual zone rows in place. This keeps `zone_index` renumbering (spec §5 "Re-numbering on edit") a pure write, and avoids inconsistent intermediate states.

- [ ] **Step 1: Implement `layouts.rs` with tests**

```rust
// crates/gnome-zones-daemon/src/db/layouts.rs
use crate::db::Database;
use crate::error::Result;
use crate::model::{Layout, LayoutSummary, ZoneRect};
use rusqlite::{params, OptionalExtension};

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn create_layout(
    db: &mut Database,
    name: &str,
    is_preset: bool,
    zones: &[ZoneRect],
) -> Result<i64> {
    let tx = db.conn.transaction()?;
    tx.execute(
        "INSERT INTO layouts (name, is_preset, created_at) VALUES (?1, ?2, ?3)",
        params![name, is_preset as i32, now_unix()],
    )?;
    let layout_id = tx.last_insert_rowid();
    for z in zones {
        tx.execute(
            "INSERT INTO zones (layout_id, zone_index, x, y, w, h)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![layout_id, z.zone_index, z.x, z.y, z.w, z.h],
        )?;
    }
    tx.commit()?;
    Ok(layout_id)
}

pub fn update_layout(
    db: &mut Database,
    id: i64,
    name: &str,
    zones: &[ZoneRect],
) -> Result<()> {
    let tx = db.conn.transaction()?;
    let is_preset: i32 = tx.query_row(
        "SELECT is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_preset == 1 {
        return Err(crate::error::Error::Config(
            "cannot modify a preset layout — fork with Save As first".into(),
        ));
    }
    tx.execute("UPDATE layouts SET name = ?1 WHERE id = ?2", params![name, id])?;
    tx.execute("DELETE FROM zones WHERE layout_id = ?1", params![id])?;
    for z in zones {
        tx.execute(
            "INSERT INTO zones (layout_id, zone_index, x, y, w, h)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, z.zone_index, z.x, z.y, z.w, z.h],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_layout(db: &mut Database, id: i64) -> Result<()> {
    let is_preset: i32 = db.conn.query_row(
        "SELECT is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_preset == 1 {
        return Err(crate::error::Error::Config("cannot delete a preset layout".into()));
    }
    db.conn.execute("DELETE FROM layouts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_layout(db: &Database, id: i64) -> Result<Option<Layout>> {
    let row = db.conn.query_row(
        "SELECT id, name, is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i32>(2)?)),
    ).optional()?;
    let Some((id, name, is_preset)) = row else { return Ok(None) };

    let mut stmt = db.conn.prepare(
        "SELECT zone_index, x, y, w, h FROM zones WHERE layout_id = ?1 ORDER BY zone_index",
    )?;
    let zones: Vec<ZoneRect> = stmt
        .query_map(params![id], |r| {
            Ok(ZoneRect {
                zone_index: r.get::<_, i64>(0)? as u32,
                x: r.get(1)?,
                y: r.get(2)?,
                w: r.get(3)?,
                h: r.get(4)?,
            })
        })?
        .map(|r| r.map_err(crate::error::Error::from))
        .collect::<Result<_>>()?;

    Ok(Some(Layout { id, name, is_preset: is_preset == 1, zones }))
}

pub fn list_layouts(db: &Database) -> Result<Vec<LayoutSummary>> {
    let mut stmt = db.conn.prepare(
        "SELECT l.id, l.name, l.is_preset,
                (SELECT COUNT(*) FROM zones z WHERE z.layout_id = l.id) AS zone_count
         FROM layouts l
         ORDER BY l.is_preset DESC, l.name ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(LayoutSummary {
            id: r.get(0)?,
            name: r.get(1)?,
            is_preset: r.get::<_, i32>(2)? == 1,
            zone_count: r.get::<_, i64>(3)? as u32,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::temp_db;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn create_and_get_roundtrips() {
        let mut db = temp_db();
        let id = create_layout(
            &mut db, "Two Columns", false,
            &[
                z(1, 0.0, 0.0, 0.5, 1.0),
                z(2, 0.5, 0.0, 0.5, 1.0),
            ],
        ).unwrap();
        let layout = get_layout(&db, id).unwrap().unwrap();
        assert_eq!(layout.name, "Two Columns");
        assert_eq!(layout.zones.len(), 2);
        assert_eq!(layout.zones[0].zone_index, 1);
        assert_eq!(layout.zones[1].zone_index, 2);
    }

    #[test]
    fn update_replaces_zone_list_atomically() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "X", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        update_layout(&mut db, id, "Y", &[
            z(1, 0.0, 0.0, 0.5, 0.5),
            z(2, 0.5, 0.0, 0.5, 0.5),
            z(3, 0.0, 0.5, 1.0, 0.5),
        ]).unwrap();
        let layout = get_layout(&db, id).unwrap().unwrap();
        assert_eq!(layout.name, "Y");
        assert_eq!(layout.zones.len(), 3);
    }

    #[test]
    fn delete_removes_zones_cascade() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "doomed", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        delete_layout(&mut db, id).unwrap();
        assert!(get_layout(&db, id).unwrap().is_none());
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM zones WHERE layout_id = ?1", params![id], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn cannot_delete_preset() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let err = delete_layout(&mut db, id).unwrap_err();
        assert!(err.to_string().contains("preset"));
    }

    #[test]
    fn cannot_update_preset() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let err = update_layout(&mut db, id, "hacked", &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap_err();
        assert!(err.to_string().contains("preset"));
    }

    #[test]
    fn list_sorts_presets_first() {
        let mut db = temp_db();
        let _user  = create_layout(&mut db, "AAA user",  false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let _preset = create_layout(&mut db, "ZZZ preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let list = list_layouts(&db).unwrap();
        assert_eq!(list[0].name, "ZZZ preset");
        assert_eq!(list[1].name, "AAA user");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p gnome-zones-daemon db::layouts
```

Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/db/layouts.rs
git commit -m "feat(zones-daemon): layout CRUD with preset protection"
```

---

## Task 8: Monitor assignment CRUD

**Files:**
- Modify: `crates/gnome-zones-daemon/src/db/monitors.rs`

- [ ] **Step 1: Implement `monitors.rs`**

```rust
// crates/gnome-zones-daemon/src/db/monitors.rs
use crate::db::Database;
use crate::error::Result;
use rusqlite::{params, OptionalExtension};

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn assign_layout(db: &Database, monitor_key: &str, layout_id: i64) -> Result<()> {
    db.conn.execute(
        "INSERT INTO monitor_assignments (monitor_key, layout_id, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(monitor_key) DO UPDATE SET layout_id = excluded.layout_id,
                                                updated_at = excluded.updated_at",
        params![monitor_key, layout_id, now_unix()],
    )?;
    Ok(())
}

pub fn get_assigned_layout_id(db: &Database, monitor_key: &str) -> Result<Option<i64>> {
    let id: Option<i64> = db.conn.query_row(
        "SELECT layout_id FROM monitor_assignments WHERE monitor_key = ?1",
        params![monitor_key],
        |r| r.get(0),
    ).optional()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::layouts::create_layout;
    use crate::db::tests::temp_db;
    use crate::model::ZoneRect;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn assign_and_retrieve() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "L", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        assign_layout(&db, "DP-1:abc123", id).unwrap();
        assert_eq!(get_assigned_layout_id(&db, "DP-1:abc123").unwrap(), Some(id));
    }

    #[test]
    fn unassigned_monitor_returns_none() {
        let db = temp_db();
        assert_eq!(get_assigned_layout_id(&db, "never-plugged").unwrap(), None);
    }

    #[test]
    fn assigning_same_monitor_twice_overwrites() {
        let mut db = temp_db();
        let a = create_layout(&mut db, "A", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let b = create_layout(&mut db, "B", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        assign_layout(&db, "DP-1", a).unwrap();
        assign_layout(&db, "DP-1", b).unwrap();
        assert_eq!(get_assigned_layout_id(&db, "DP-1").unwrap(), Some(b));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p gnome-zones-daemon db::monitors
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/db/monitors.rs
git commit -m "feat(zones-daemon): per-monitor layout assignment"
```

---

## Task 9: Settings CRUD

**Files:**
- Modify: `crates/gnome-zones-daemon/src/db/settings.rs`

- [ ] **Step 1: Implement `settings.rs`**

```rust
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p gnome-zones-daemon db::settings
```

Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/db/settings.rs
git commit -m "feat(zones-daemon): key/value settings with typed accessors"
```

---

## Task 10: Preset definitions + seeding

**Files:**
- Create: `crates/gnome-zones-daemon/src/presets.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

Seed the 8 presets listed in spec §2. Seeding is idempotent: we check for the existence of each preset by name before inserting.

- [ ] **Step 1: Create `src/presets.rs`**

```rust
// crates/gnome-zones-daemon/src/presets.rs
use crate::db::{layouts, Database};
use crate::error::Result;
use crate::model::ZoneRect;
use rusqlite::params;

fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
    ZoneRect { zone_index: i, x, y, w, h }
}

/// Name + zone list for each built-in preset.
///
/// Numbering is row-major (top-to-bottom, left-to-right by top-left corner).
pub fn builtin_presets() -> Vec<(&'static str, Vec<ZoneRect>)> {
    vec![
        ("Two Columns (50/50)", vec![
            z(1, 0.0, 0.0, 0.5, 1.0),
            z(2, 0.5, 0.0, 0.5, 1.0),
        ]),
        ("Three Columns", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
            z(3, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("2/3 | 1/3", vec![
            z(1, 0.0,         0.0, 2.0 / 3.0, 1.0),
            z(2, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("1/3 | 2/3", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 2.0 / 3.0, 1.0),
        ]),
        ("2×2 Grid", vec![
            z(1, 0.0, 0.0, 0.5, 0.5),
            z(2, 0.5, 0.0, 0.5, 0.5),
            z(3, 0.0, 0.5, 0.5, 0.5),
            z(4, 0.5, 0.5, 0.5, 0.5),
        ]),
        ("1/3 | 1/3 | 1/3", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
            z(3, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("Sidebar + Main (1/4 | 3/4)", vec![
            z(1, 0.0,  0.0, 0.25, 1.0),
            z(2, 0.25, 0.0, 0.75, 1.0),
        ]),
        ("Main + Sidebar (3/4 | 1/4)", vec![
            z(1, 0.0,  0.0, 0.75, 1.0),
            z(2, 0.75, 0.0, 0.25, 1.0),
        ]),
    ]
}

/// Idempotent — safe to call on every daemon start.
pub fn seed(db: &mut Database) -> Result<()> {
    for (name, zones) in builtin_presets() {
        let exists: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE name = ?1 AND is_preset = 1",
            params![name],
            |r| r.get(0),
        )?;
        if exists == 0 {
            layouts::create_layout(db, name, true, &zones)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::temp_db;

    #[test]
    fn every_preset_covers_exactly_unit_area() {
        for (name, zones) in builtin_presets() {
            let total: f64 = zones.iter().map(|z| z.w * z.h).sum();
            assert!((total - 1.0).abs() < 1e-9, "{name} does not tile the unit square ({total})");
        }
    }

    #[test]
    fn every_preset_uses_sequential_indices() {
        for (name, zones) in builtin_presets() {
            for (i, z) in zones.iter().enumerate() {
                assert_eq!(z.zone_index as usize, i + 1, "{name} zone {i}");
            }
        }
    }

    #[test]
    fn seed_populates_all_presets() {
        let mut db = temp_db();
        seed(&mut db).unwrap();
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE is_preset = 1", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count as usize, builtin_presets().len());
    }

    #[test]
    fn seed_is_idempotent() {
        let mut db = temp_db();
        seed(&mut db).unwrap();
        seed(&mut db).unwrap();
        seed(&mut db).unwrap();
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE is_preset = 1", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count as usize, builtin_presets().len());
    }
}
```

- [ ] **Step 2: Declare module**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod error;
mod math;
mod model;
mod presets;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon presets
```

Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): default preset layouts + idempotent seeding"
```

---

## Task 11: Monitor enumeration + `monitor_key`

**Files:**
- Create: `crates/gnome-zones-daemon/src/monitors.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

`MonitorService` talks to `org.gnome.Mutter.DisplayConfig`. We use the `GetCurrentState` method — its return struct contains the full monitor list with connector name, resolution, and EDID blob.

The **`monitor_key`** is `"<connector>:<edid_hash8>"` where `edid_hash8` is the first 8 hex characters of `sha256(edid)`. If the EDID is empty (rare; usually VMs or very old displays), we fall back to `"<connector>:no-edid"`.

Because `GetCurrentState`'s return signature is deeply nested, we wrap the zbus proxy behind our own typed `MonitorService` trait. This keeps the nested Variant parsing contained and lets us swap in a mock service for testing the rest of the daemon.

- [ ] **Step 1: Create `src/monitors.rs`**

```rust
// crates/gnome-zones-daemon/src/monitors.rs
use crate::error::Result;
use crate::model::MonitorInfo;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use zbus::{proxy, Connection};

/// Trait over the source of monitor information. Real implementation hits
/// `org.gnome.Mutter.DisplayConfig`; tests inject a mock.
#[async_trait]
pub trait MonitorService: Send + Sync {
    async fn list_monitors(&self) -> Result<Vec<MonitorInfo>>;
}

pub fn compute_monitor_key(connector: &str, edid: &[u8]) -> String {
    if edid.is_empty() {
        format!("{connector}:no-edid")
    } else {
        let digest = Sha256::digest(edid);
        let short = hex::encode(&digest[..4]);  // 8 hex chars
        format!("{connector}:{short}")
    }
}

// ---- Real Mutter-backed implementation ----

#[proxy(
    interface    = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path    = "/org/gnome/Mutter/DisplayConfig"
)]
trait MutterDisplayConfig {
    /// Returns (serial, monitors, logical_monitors, properties).
    /// We only use `monitors` and `logical_monitors`.
    fn get_current_state(&self) -> zbus::Result<(
        u32,
        Vec<MutterMonitor>,
        Vec<MutterLogicalMonitor>,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    )>;
}

type MutterMonitor = (
    (String, String, String, String),   // connector, vendor, product, serial
    Vec<(String, i32, i32, f64, f64, Vec<f64>, std::collections::HashMap<String, zbus::zvariant::OwnedValue>)>,
    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,  // props (edid is here as "edid" Vec<u8>)
);

type MutterLogicalMonitor = (
    i32, i32,   // x, y
    f64,        // scale
    u32,        // transform
    bool,       // primary
    Vec<(String, String, String, String)>,  // monitors assigned
    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
);

pub struct MutterMonitorService {
    proxy: MutterDisplayConfigProxy<'static>,
}

impl MutterMonitorService {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self {
            proxy: MutterDisplayConfigProxy::new(conn).await?,
        })
    }
}

#[async_trait]
impl MonitorService for MutterMonitorService {
    async fn list_monitors(&self) -> Result<Vec<MonitorInfo>> {
        let (_serial, monitors, logical_monitors, _props) =
            self.proxy.get_current_state().await?;

        let mut out = Vec::with_capacity(monitors.len());
        for ((connector, _vendor, _product, _serial), modes, props) in monitors.iter() {
            let edid: Vec<u8> = props
                .get("edid")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| Vec::<u8>::try_from(v).ok())
                .unwrap_or_default();
            let monitor_key = compute_monitor_key(connector, &edid);

            // Pick the current (active) mode — the one flagged "is-current".
            let (width_px, height_px) = modes
                .iter()
                .find_map(|(_id, w, h, _rate, _scale, _supported, flags)| {
                    flags.get("is-current")
                        .and_then(|v| bool::try_from(v.try_clone().ok()?).ok())
                        .filter(|b| *b)
                        .map(|_| (*w as u32, *h as u32))
                })
                .unwrap_or((0, 0));

            let is_primary = logical_monitors.iter().any(|(_x, _y, _s, _t, primary, assigned, _)| {
                *primary && assigned.iter().any(|(c, _, _, _)| c == connector)
            });

            let name = props.get("display-name")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| String::try_from(v).ok())
                .unwrap_or_else(|| connector.clone());

            out.push(MonitorInfo {
                monitor_key,
                connector: connector.clone(),
                name,
                width_px,
                height_px,
                is_primary,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_with_edid_is_connector_plus_hex8() {
        let edid = b"monitor edid payload";
        let k = compute_monitor_key("DP-1", edid);
        assert!(k.starts_with("DP-1:"));
        assert_eq!(k.split(':').nth(1).unwrap().len(), 8);
    }

    #[test]
    fn key_without_edid_falls_back() {
        assert_eq!(compute_monitor_key("HDMI-2", &[]), "HDMI-2:no-edid");
    }

    #[test]
    fn identical_edid_produces_identical_key() {
        let edid = b"AAAA";
        assert_eq!(compute_monitor_key("DP-1", edid), compute_monitor_key("DP-1", edid));
    }

    #[test]
    fn different_edid_produces_different_key() {
        assert_ne!(compute_monitor_key("DP-1", b"A"), compute_monitor_key("DP-1", b"B"));
    }
}
```

- [ ] **Step 2: Add async-trait dependency**

Modify `crates/gnome-zones-daemon/Cargo.toml`, adding to `[dependencies]`:

```toml
async-trait = "0.1"
```

- [ ] **Step 3: Declare module in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod error;
mod math;
mod model;
mod monitors;
mod presets;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 4: Verify compile and run unit tests**

```bash
cargo build -p gnome-zones-daemon
cargo test  -p gnome-zones-daemon monitors
```

Expected: build succeeds, 4 tests pass. (The zbus proxy machinery is only touched when `MutterMonitorService` is instantiated, which the tests don't do.)

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones-daemon
git commit -m "feat(zones-daemon): DisplayConfig monitor enumeration + monitor_key"
```

---

## Task 12: Shell-extension shim (gnome-zones-mover)

**Files:**
- Create: `dist/shell-extension/gnome-zones-mover@power-toys/metadata.json`
- Create: `dist/shell-extension/gnome-zones-mover@power-toys/extension.js`

Per spec §7: the shim is a pure Mutter API bridge. No logic — just a D-Bus service exposing `MoveResizeWindow(window_id, x, y, w, h)` that calls `Meta.Window.move_resize_frame()`.

- [ ] **Step 1: Create `metadata.json`**

```json
{
    "uuid": "gnome-zones-mover@power-toys",
    "name": "gnome-zones Window Mover",
    "description": "Mutter API bridge for gnome-zones-daemon. No UI. Not intended for standalone use.",
    "shell-version": ["45", "46", "47"],
    "url": "https://github.com/gnome-power-toys/gnome-zones",
    "settings-schema": null
}
```

- [ ] **Step 2: Create `extension.js`**

```javascript
// dist/shell-extension/gnome-zones-mover@power-toys/extension.js
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';

const DBUS_IFACE = `
<node>
  <interface name="org.gnome.Shell.Extensions.GnomeZonesMover">
    <method name="MoveResizeWindow">
      <arg type="t" direction="in" name="window_id" />
      <arg type="i" direction="in" name="x" />
      <arg type="i" direction="in" name="y" />
      <arg type="i" direction="in" name="w" />
      <arg type="i" direction="in" name="h" />
      <arg type="b" direction="out" name="ok" />
    </method>
    <method name="GetFocusedWindowId">
      <arg type="t" direction="out" name="window_id" />
    </method>
    <method name="ListWindowsInRect">
      <arg type="i" direction="in" name="x" />
      <arg type="i" direction="in" name="y" />
      <arg type="i" direction="in" name="w" />
      <arg type="i" direction="in" name="h" />
      <arg type="at" direction="out" name="window_ids" />
    </method>
    <method name="ActivateWindow">
      <arg type="t" direction="in" name="window_id" />
    </method>
  </interface>
</node>
`;

export default class GnomeZonesMoverExtension {
    constructor(metadata) {
        this._metadata = metadata;
        this._impl = null;
    }

    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(DBUS_IFACE, this);
        this._impl.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/GnomeZonesMover');
        log('[gnome-zones-mover] enabled');
    }

    disable() {
        if (this._impl) {
            this._impl.unexport();
            this._impl = null;
        }
        log('[gnome-zones-mover] disabled');
    }

    // --- D-Bus methods ---

    MoveResizeWindow(window_id, x, y, w, h) {
        const win = this._findWindow(window_id);
        if (!win) return false;
        try {
            // Unmaximize first — spec §4. Otherwise move_resize_frame is ignored.
            if (win.get_maximized()) {
                win.unmaximize(Meta.MaximizeFlags.BOTH);
            }
            if (win.is_fullscreen()) {
                win.unmake_fullscreen();
            }
            // `true` = user-resize, so GTK clients pick up the new size.
            win.move_resize_frame(true, x, y, w, h);
            return true;
        } catch (e) {
            logError(e, '[gnome-zones-mover] MoveResizeWindow failed');
            return false;
        }
    }

    GetFocusedWindowId() {
        const win = global.display.focus_window;
        return win ? win.get_id() : 0;
    }

    ListWindowsInRect(x, y, w, h) {
        const actors = global.get_window_actors();
        const x1 = x + w, y1 = y + h;
        return actors
            .map(a => a.meta_window)
            .filter(w => w && !w.is_hidden() && !w.minimized)
            .filter(w => {
                const r = w.get_frame_rect();
                const cx = r.x + r.width  / 2;
                const cy = r.y + r.height / 2;
                return cx >= x && cx < x1 && cy >= y && cy < y1;
            })
            .map(w => w.get_id());
    }

    ActivateWindow(window_id) {
        const win = this._findWindow(window_id);
        if (win) {
            win.activate(global.get_current_time());
        }
    }

    // --- helpers ---

    _findWindow(id) {
        return global.get_window_actors()
            .map(a => a.meta_window)
            .find(w => w && w.get_id() === id) || null;
    }
}
```

- [ ] **Step 3: Verify the extension is syntactically valid**

There's no build step for GJS extensions; the best static check is the syntax parser GJS ships:

```bash
gjs --check dist/shell-extension/gnome-zones-mover@power-toys/extension.js
```

Expected: no output (exit 0). If `gjs` is not installed, skip this check — it'll be exercised when the extension loads.

- [ ] **Step 4: Commit**

```bash
git add dist/shell-extension/
git commit -m "feat(zones): shell-extension shim — Mutter move-resize bridge"
```

---

## Task 13: `WindowMover` trait + `ShimMover` implementation

**Files:**
- Create: `crates/gnome-zones-daemon/src/window/mod.rs`
- Create: `crates/gnome-zones-daemon/src/window/shim.rs`
- Create: `crates/gnome-zones-daemon/src/window/mutter.rs` (stub for Task 14)
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Create `src/window/mod.rs`**

```rust
// crates/gnome-zones-daemon/src/window/mod.rs
use crate::error::Result;
use crate::math::PixelRect;
use async_trait::async_trait;

pub mod mutter;
pub mod shim;

/// The daemon uses this trait for every interaction with windows — move/resize,
/// focus resolution, window lookup by rect. Production wires the `ShimMover`
/// (our extension) as primary with `MutterMover` as fallback; tests inject a mock.
#[async_trait]
pub trait WindowMover: Send + Sync {
    async fn focused_window_id(&self) -> Result<u64>;
    async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()>;
    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>>;
    async fn activate(&self, window_id: u64) -> Result<()>;
}
```

- [ ] **Step 2: Create `src/window/shim.rs`**

```rust
// crates/gnome-zones-daemon/src/window/shim.rs
use crate::error::{Error, Result};
use crate::math::PixelRect;
use crate::window::WindowMover;
use async_trait::async_trait;
use zbus::{proxy, Connection};

#[proxy(
    interface        = "org.gnome.Shell.Extensions.GnomeZonesMover",
    default_service  = "org.gnome.Shell",
    default_path     = "/org/gnome/Shell/Extensions/GnomeZonesMover"
)]
trait GnomeZonesMover {
    fn move_resize_window(&self, window_id: u64, x: i32, y: i32, w: i32, h: i32) -> zbus::Result<bool>;
    fn get_focused_window_id(&self) -> zbus::Result<u64>;
    fn list_windows_in_rect(&self, x: i32, y: i32, w: i32, h: i32) -> zbus::Result<Vec<u64>>;
    fn activate_window(&self, window_id: u64) -> zbus::Result<()>;
}

pub struct ShimMover {
    proxy: GnomeZonesMoverProxy<'static>,
}

impl ShimMover {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self { proxy: GnomeZonesMoverProxy::new(conn).await? })
    }
}

#[async_trait]
impl WindowMover for ShimMover {
    async fn focused_window_id(&self) -> Result<u64> {
        let id = self.proxy.get_focused_window_id().await?;
        if id == 0 {
            return Err(Error::NoFocusedWindow);
        }
        Ok(id)
    }

    async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()> {
        let ok = self.proxy.move_resize_window(window_id, rect.x, rect.y, rect.w, rect.h).await?;
        if !ok {
            return Err(Error::Compositor(format!("mover rejected move_resize for {window_id}")));
        }
        Ok(())
    }

    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>> {
        Ok(self.proxy.list_windows_in_rect(rect.x, rect.y, rect.w, rect.h).await?)
    }

    async fn activate(&self, window_id: u64) -> Result<()> {
        Ok(self.proxy.activate_window(window_id).await?)
    }
}
```

- [ ] **Step 3: Create stub `src/window/mutter.rs`**

```rust
// crates/gnome-zones-daemon/src/window/mutter.rs
// Implemented in Task 14.
```

- [ ] **Step 4: Declare module in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod error;
mod math;
mod model;
mod monitors;
mod presets;
mod window;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 5: Build**

```bash
cargo build -p gnome-zones-daemon
```

Expected: compiles (no new tests — this task just wires types).

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): WindowMover trait + ShimMover (extension-backed)"
```

---

## Task 14: `MutterMover` fallback

**Files:**
- Modify: `crates/gnome-zones-daemon/src/window/mutter.rs`

When `gnome-zones-mover@power-toys` isn't enabled, we fall back to Mutter's own introspection. It's more limited (no cheap windows-in-rect query), but move-resize and focused-window resolution work.

Mutter's `org.gnome.Shell.Introspect` provides `GetWindows` (returning `(a{ta{sv}})` — a map of window id → properties including `frame-rect`). It doesn't expose move-resize directly, but `org.gnome.Mutter.WindowMover` (where present) does. For systems where neither is available, the daemon logs and errors out with a clear "please enable the `gnome-zones-mover` extension" message.

- [ ] **Step 1: Implement `mutter.rs`**

```rust
// crates/gnome-zones-daemon/src/window/mutter.rs
use crate::error::{Error, Result};
use crate::math::PixelRect;
use crate::window::WindowMover;
use async_trait::async_trait;
use zbus::{proxy, Connection};

#[proxy(
    interface       = "org.gnome.Shell.Introspect",
    default_service = "org.gnome.Shell",
    default_path    = "/org/gnome/Shell/Introspect"
)]
trait ShellIntrospect {
    fn get_windows(&self) -> zbus::Result<
        std::collections::HashMap<u64, std::collections::HashMap<String, zbus::zvariant::OwnedValue>>
    >;
}

pub struct MutterMover {
    introspect: ShellIntrospectProxy<'static>,
}

impl MutterMover {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self { introspect: ShellIntrospectProxy::new(conn).await? })
    }
}

#[async_trait]
impl WindowMover for MutterMover {
    async fn focused_window_id(&self) -> Result<u64> {
        let windows = self.introspect.get_windows().await?;
        for (id, props) in windows {
            if let Some(v) = props.get("has-focus") {
                if let Ok(b) = bool::try_from(v.try_clone()?) {
                    if b {
                        return Ok(id);
                    }
                }
            }
        }
        Err(Error::NoFocusedWindow)
    }

    async fn move_resize(&self, _window_id: u64, _rect: PixelRect) -> Result<()> {
        // No mainline D-Bus API exists to move other apps' windows without a
        // Shell extension. Surface a clear error so callers can fall back.
        Err(Error::Compositor(
            "move_resize unavailable without gnome-zones-mover shell extension".into()
        ))
    }

    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>> {
        let windows = self.introspect.get_windows().await?;
        let mut out = Vec::new();
        let x1 = rect.x + rect.w;
        let y1 = rect.y + rect.h;
        for (id, props) in windows {
            let Some(v) = props.get("frame-rect") else { continue };
            // frame-rect is an (iiii) tuple
            let Ok(tuple) = <(i32, i32, i32, i32)>::try_from(v.try_clone()?) else { continue };
            let cx = tuple.0 + tuple.2 / 2;
            let cy = tuple.1 + tuple.3 / 2;
            if cx >= rect.x && cx < x1 && cy >= rect.y && cy < y1 {
                out.push(id);
            }
        }
        Ok(out)
    }

    async fn activate(&self, _window_id: u64) -> Result<()> {
        Err(Error::Compositor(
            "activate unavailable without gnome-zones-mover shell extension".into()
        ))
    }
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p gnome-zones-daemon
```

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/window/mutter.rs
git commit -m "feat(zones-daemon): MutterMover fallback (introspect-only, read-mostly)"
```

---

## Task 15: Snap-engine state map

**Files:**
- Create: `crates/gnome-zones-daemon/src/snap/mod.rs`
- Create: `crates/gnome-zones-daemon/src/snap/state.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

We track, per window: the pre-snap rect (for v2's unsnap-to-original), and the set of zone indices the window is currently snapped across.

- [ ] **Step 1: Create `src/snap/state.rs`**

```rust
// crates/gnome-zones-daemon/src/snap/state.rs
use crate::math::PixelRect;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct WindowState {
    /// Rect the window had right before its first snap — used for v2 unsnap.
    pub pre_snap: Option<PixelRect>,
    /// Zone indices the window is currently snapped across (empty = not snapped).
    pub zones: Vec<u32>,
}

#[derive(Default)]
pub struct WindowStateMap(Mutex<HashMap<u64, WindowState>>);

impl WindowStateMap {
    pub fn new() -> Self { Self::default() }

    pub async fn get(&self, id: u64) -> WindowState {
        self.0.lock().await.get(&id).cloned().unwrap_or_default()
    }

    pub async fn set_zones(&self, id: u64, zones: Vec<u32>) {
        self.0.lock().await.entry(id).or_default().zones = zones;
    }

    pub async fn ensure_pre_snap(&self, id: u64, rect: PixelRect) {
        let mut map = self.0.lock().await;
        let entry = map.entry(id).or_default();
        if entry.pre_snap.is_none() {
            entry.pre_snap = Some(rect);
        }
    }

    pub async fn forget(&self, id: u64) {
        self.0.lock().await.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_on_unknown_id_is_default() {
        let m = WindowStateMap::new();
        let s = m.get(42).await;
        assert!(s.zones.is_empty());
        assert!(s.pre_snap.is_none());
    }

    #[tokio::test]
    async fn set_and_retrieve_zones() {
        let m = WindowStateMap::new();
        m.set_zones(7, vec![2, 3]).await;
        let s = m.get(7).await;
        assert_eq!(s.zones, vec![2, 3]);
    }

    #[tokio::test]
    async fn ensure_pre_snap_only_sets_once() {
        let m = WindowStateMap::new();
        let r1 = PixelRect { x: 0, y: 0, w: 100, h: 100 };
        let r2 = PixelRect { x: 50, y: 50, w: 100, h: 100 };
        m.ensure_pre_snap(7, r1).await;
        m.ensure_pre_snap(7, r2).await;
        assert_eq!(m.get(7).await.pre_snap, Some(r1));
    }

    #[tokio::test]
    async fn forget_removes_entry() {
        let m = WindowStateMap::new();
        m.set_zones(7, vec![1]).await;
        m.forget(7).await;
        assert!(m.get(7).await.zones.is_empty());
    }
}
```

- [ ] **Step 2: Create stub `src/snap/mod.rs`**

```rust
// crates/gnome-zones-daemon/src/snap/mod.rs
pub mod state;

// SnapEngine implemented in Task 16+.
```

- [ ] **Step 3: Declare module in main.rs**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod error;
mod math;
mod model;
mod monitors;
mod presets;
mod snap;
mod window;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones-daemon snap
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): per-window snap-state map"
```

---

## Task 16: `SnapEngine` skeleton + active-layout resolution

**Files:**
- Modify: `crates/gnome-zones-daemon/src/snap/mod.rs`

`SnapEngine` is the single place where "which monitor is the focused window on, what zones apply" is resolved. Every action method (snap, iterate, cycle) starts here.

For v1 single-monitor UX scope, we always target the primary monitor. `MonitorInfo` doesn't yet carry logical origin, and routing windows to the monitor they actually live on requires that. Plan 2 extends this when the UI lands.

- [ ] **Step 1: Implement the struct and helpers in `snap/mod.rs`**

```rust
// crates/gnome-zones-daemon/src/snap/mod.rs
pub mod state;

use crate::db::{layouts, monitors, Database};
use crate::error::{Error, Result};
use crate::math::{self, PixelRect};
use crate::model::{Layout, MonitorInfo};
use crate::monitors::MonitorService;
use crate::snap::state::WindowStateMap;
use crate::window::WindowMover;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SnapEngine {
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) monitor_svc: Arc<dyn MonitorService>,
    pub(crate) mover: Arc<dyn WindowMover>,
    pub(crate) states: Arc<WindowStateMap>,
}

impl SnapEngine {
    pub fn new(
        db: Arc<Mutex<Database>>,
        monitor_svc: Arc<dyn MonitorService>,
        mover: Arc<dyn WindowMover>,
        states: Arc<WindowStateMap>,
    ) -> Self {
        Self { db, monitor_svc, mover, states }
    }

    /// Pick the monitor to act on. Prefers primary, falls back to the first in
    /// the list. v1 keyboard-snap scope: always pick one monitor regardless of
    /// window position (spec §5 "Multi-monitor in v1").
    pub(crate) async fn target_monitor(&self) -> Result<MonitorInfo> {
        let mut monitors = self.monitor_svc.list_monitors().await?;
        monitors.sort_by_key(|m| !m.is_primary);  // primary first
        monitors.into_iter().next()
            .ok_or_else(|| Error::Compositor("no monitors enumerated".into()))
    }

    /// Fetch the layout assigned to a monitor. If none assigned, falls back to
    /// "Two Columns (50/50)" and persists the assignment for next time.
    pub(crate) async fn active_layout_for(&self, monitor_key: &str) -> Result<Layout> {
        let db = self.db.lock().await;
        let layout_id = match monitors::get_assigned_layout_id(&db, monitor_key)? {
            Some(id) => id,
            None => {
                let fallback: i64 = db.conn.query_row(
                    "SELECT id FROM layouts WHERE name = 'Two Columns (50/50)' AND is_preset = 1",
                    [],
                    |r| r.get(0),
                )?;
                monitors::assign_layout(&db, monitor_key, fallback)?;
                fallback
            }
        };
        layouts::get_layout(&db, layout_id)?
            .ok_or_else(|| Error::NoLayoutForMonitor(monitor_key.into()))
    }
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p gnome-zones-daemon
```

Expected: compiles. No new tests — this task wires resolution helpers used by the next tasks.

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/snap/mod.rs
git commit -m "feat(zones-daemon): SnapEngine struct + monitor/layout resolution"
```

---

## Task 17: `SnapEngine::snap_focused_to_zone` (no-span path)

**Files:**
- Modify: `crates/gnome-zones-daemon/src/snap/mod.rs`

Implements `SnapFocusedToZone(zone_index, span = false)` end-to-end.

- [ ] **Step 1: Add the method + a mock-backed test**

Add to `impl SnapEngine` in `snap/mod.rs`:

```rust
    /// Snap the focused window to zone `zone_index`.
    /// `span = true` adds the zone to the window's current span set;
    /// `span = false` replaces whatever set was there.
    pub async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> Result<()> {
        // Respect pause.
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused {
            tracing::debug!(zone_index, "paused — ignoring snap");
            return Ok(());
        }

        let win_id = self.mover.focused_window_id().await?;
        let monitor = self.target_monitor().await?;

        let layout = self.active_layout_for(&monitor.monitor_key).await?;
        let zone_count = layout.zones.len() as u32;
        if zone_index == 0 || zone_index > zone_count {
            return Err(Error::InvalidZoneIndex(zone_index, zone_count));
        }

        // Compute the target set of zone indices.
        let current_state = self.states.get(win_id).await;
        let mut zone_set: Vec<u32> = if span {
            current_state.zones.clone()
        } else {
            Vec::new()
        };
        if !zone_set.contains(&zone_index) {
            zone_set.push(zone_index);
        }
        zone_set.sort_unstable();
        zone_set.dedup();

        // Resolve the target pixel rect (union of the set, deflated by gap).
        let zones: Vec<&_> = zone_set
            .iter()
            .filter_map(|i| layout.zone(*i))
            .collect();
        let union_frac = math::bounding_rect(&zones);

        let gap = {
            let db = self.db.lock().await;
            crate::db::settings::get_int(&db, "gap_px", 8)? as i32
        };
        let target_px = math::deflate(
            math::project_rect(&union_frac, monitor.width_px as i32, monitor.height_px as i32),
            gap,
        );

        // Persist pre-snap rect on first snap. v1 has no "unsnap to original"
        // UI (spec §11) — we record target rect as a placeholder so the state
        // entry exists; Plan 2 will add a `get_window_frame` to WindowMover
        // for the real pre-snap rect.
        if current_state.zones.is_empty() {
            self.states.ensure_pre_snap(win_id, target_px).await;
        }

        // Execute the move-resize.
        self.mover.move_resize(win_id, target_px).await?;
        self.states.set_zones(win_id, zone_set).await;
        Ok(())
    }
```

- [ ] **Step 2: Add a testing harness with mock services**

The tests keep their own `Arc<MockMover>` outside the engine, avoiding any `downcast_ref` acrobatics. Add at the bottom of `src/snap/mod.rs`:

```rust
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::model::MonitorInfo;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;
    use tempfile::NamedTempFile;

    pub struct MockMonitor {
        pub monitors: Vec<MonitorInfo>,
    }

    #[async_trait]
    impl MonitorService for MockMonitor {
        async fn list_monitors(&self) -> Result<Vec<MonitorInfo>> {
            Ok(self.monitors.clone())
        }
    }

    #[derive(Default)]
    pub struct MockMover {
        pub focused: StdMutex<u64>,
        pub moves: StdMutex<Vec<(u64, PixelRect)>>,
        pub activations: StdMutex<Vec<u64>>,
        pub windows_in_rect_result: StdMutex<Vec<u64>>,
    }

    #[async_trait]
    impl WindowMover for MockMover {
        async fn focused_window_id(&self) -> Result<u64> {
            let id = *self.focused.lock().unwrap();
            if id == 0 { Err(Error::NoFocusedWindow) } else { Ok(id) }
        }
        async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()> {
            self.moves.lock().unwrap().push((window_id, rect));
            Ok(())
        }
        async fn windows_in_rect(&self, _rect: PixelRect) -> Result<Vec<u64>> {
            Ok(self.windows_in_rect_result.lock().unwrap().clone())
        }
        async fn activate(&self, window_id: u64) -> Result<()> {
            self.activations.lock().unwrap().push(window_id);
            Ok(())
        }
    }

    pub fn primary_monitor(key: &str, w: u32, h: u32) -> MonitorInfo {
        MonitorInfo {
            monitor_key: key.into(),
            connector: "DP-1".into(),
            name: "Test".into(),
            width_px: w,
            height_px: h,
            is_primary: true,
        }
    }

    /// Build an engine around a pre-wrapped mover so tests can assert on the
    /// mover's internal state directly.
    pub fn temp_engine_with_mover(
        mover: Arc<MockMover>,
        monitors_vec: Vec<MonitorInfo>,
    ) -> SnapEngine {
        let f = NamedTempFile::new().unwrap();
        let mut db = Database::open(f.path()).unwrap();
        crate::presets::seed(&mut db).unwrap();
        let db = Arc::new(Mutex::new(db));
        SnapEngine::new(
            db,
            Arc::new(MockMonitor { monitors: monitors_vec }),
            mover,  // Arc<MockMover> coerces to Arc<dyn WindowMover>
            Arc::new(WindowStateMap::new()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testutil::*;

    #[tokio::test]
    async fn snap_focused_moves_window_to_zone_1() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();

        let moves = mover.moves.lock().unwrap().clone();
        assert_eq!(moves.len(), 1);
        let (id, rect) = moves[0];
        assert_eq!(id, 42);
        // "Two Columns (50/50)" zone 1 on 1920×1080 with 8px gap → x=8,y=8,w≈944,h≈1064
        assert_eq!(rect.x, 8);
        assert_eq!(rect.y, 8);
        assert!((rect.w - 944).abs() <= 1);
        assert!((rect.h - 1064).abs() <= 1);
    }

    #[tokio::test]
    async fn snap_respects_paused_setting() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        {
            let db = engine.db.lock().await;
            crate::db::settings::set_setting(&db, "paused", "true").unwrap();
        }
        engine.snap_focused_to_zone(1, false).await.unwrap();
        assert!(mover.moves.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn snap_invalid_zone_index_errors() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        let err = engine.snap_focused_to_zone(99, false).await.unwrap_err();
        assert!(matches!(err, Error::InvalidZoneIndex(99, 2)));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon snap
```

Expected: 3 new tests pass + 4 state tests from Task 15 = 7 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): snap_focused_to_zone (no-span path)"
```

---

## Task 18: Span support (`Super+Alt+N`)

**Files:**
- Modify: `crates/gnome-zones-daemon/src/snap/mod.rs`

The `span` path is already inside `snap_focused_to_zone` — we just need tests for it.

- [ ] **Step 1: Add span-path tests**

Append to `#[cfg(test)] mod tests` in `src/snap/mod.rs`:

```rust
    #[tokio::test]
    async fn span_adds_zone_to_existing_set() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        // First, snap to zone 1 only.
        engine.snap_focused_to_zone(1, false).await.unwrap();
        // Then span into zone 2.
        engine.snap_focused_to_zone(2, true).await.unwrap();

        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1, 2]);

        let moves = mover.moves.lock().unwrap().clone();
        assert_eq!(moves.len(), 2);
        // Second move's rect should span both halves.
        let (_, rect) = moves[1];
        assert!((rect.w - (1920 - 16)).abs() <= 1);  // full width minus 8px gap on each side
    }

    #[tokio::test]
    async fn non_span_replaces_set() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.snap_focused_to_zone(2, false).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p gnome-zones-daemon snap
```

Expected: 9 passed (2 new).

- [ ] **Step 3: Commit**

```bash
git add crates/gnome-zones-daemon/src/snap
git commit -m "test(zones-daemon): span path coverage"
```

---

## Task 19: `SnapEngine::iterate_focused_zone`

**Files:**
- Modify: `crates/gnome-zones-daemon/src/snap/mod.rs`

- [ ] **Step 1: Add the method to `impl SnapEngine`**

```rust
    pub async fn iterate_focused_zone(&self, dir: crate::model::IterateDir) -> Result<()> {
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused { return Ok(()); }

        let win_id = self.mover.focused_window_id().await?;
        let monitor = self.target_monitor().await?;

        let layout = self.active_layout_for(&monitor.monitor_key).await?;
        let zone_count = layout.zones.len() as u32;
        if zone_count == 0 { return Ok(()); }

        let state = self.states.get(win_id).await;
        // Treat unsnapped or multi-zone-spanning windows as index 0.
        let current_index = if state.zones.len() == 1 { state.zones[0] } else { 0 };
        let target = math::iterate_index(current_index, zone_count, dir);

        self.snap_focused_to_zone(target, false).await
    }
```

- [ ] **Step 2: Add tests**

Append to `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn iterate_next_advances_to_zone_2() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);
    }

    #[tokio::test]
    async fn iterate_next_on_unsnapped_lands_on_1() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1]);
    }

    #[tokio::test]
    async fn iterate_prev_on_unsnapped_lands_on_last() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.iterate_focused_zone(crate::model::IterateDir::Prev).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);  // "Two Columns (50/50)" has 2 zones
    }

    #[tokio::test]
    async fn iterate_next_wraps_around() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(2, false).await.unwrap();
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1]);
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon snap
```

Expected: 13 passed (4 new).

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src/snap
git commit -m "feat(zones-daemon): iterate_focused_zone (Super+Left/Right)"
```

---

## Task 20: `SnapEngine::cycle_focus_in_zone`

**Files:**
- Modify: `crates/gnome-zones-daemon/src/snap/mod.rs`

`Super+PgUp/PgDn` cycles keyboard focus between windows occupying the same zone. We:

1. Find the focused window's current zone (if any).
2. Project that zone to pixel coordinates.
3. Query `WindowMover::windows_in_rect` for the list.
4. Activate the window before (prev) or after (next) the focused one in that list.

- [ ] **Step 1: Add the method**

```rust
    pub async fn cycle_focus_in_zone(&self, direction: i32) -> Result<()> {
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused { return Ok(()); }

        let focused_id = self.mover.focused_window_id().await?;
        let state = self.states.get(focused_id).await;
        if state.zones.is_empty() {
            return Ok(());  // window not snapped — nothing to cycle.
        }

        let monitor = self.target_monitor().await?;
        let layout = self.active_layout_for(&monitor.monitor_key).await?;

        // Use union of the window's zones as the cycling rect.
        let zones_refs: Vec<&_> = state.zones.iter()
            .filter_map(|i| layout.zone(*i))
            .collect();
        if zones_refs.is_empty() { return Ok(()); }
        let union_frac = math::bounding_rect(&zones_refs);
        let rect_px = math::project_rect(
            &union_frac,
            monitor.width_px as i32,
            monitor.height_px as i32,
        );

        let ids = self.mover.windows_in_rect(rect_px).await?;
        if ids.len() < 2 { return Ok(()); }

        let pos = ids.iter().position(|&w| w == focused_id).unwrap_or(0);
        let next_pos = if direction >= 0 {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.mover.activate(ids[next_pos]).await?;
        Ok(())
    }
```

- [ ] **Step 2: Add tests**

```rust
    #[tokio::test]
    async fn cycle_activates_next_in_rect() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200, 300];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.cycle_focus_in_zone(1).await.unwrap();
        assert_eq!(mover.activations.lock().unwrap().clone(), vec![200]);
    }

    #[tokio::test]
    async fn cycle_prev_wraps_to_last() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200, 300];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.cycle_focus_in_zone(-1).await.unwrap();
        assert_eq!(mover.activations.lock().unwrap().clone(), vec![300]);
    }

    #[tokio::test]
    async fn cycle_unsnapped_is_noop() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.cycle_focus_in_zone(1).await.unwrap();
        assert!(mover.activations.lock().unwrap().is_empty());
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon snap
```

Expected: 16 passed (3 new).

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src/snap
git commit -m "feat(zones-daemon): cycle_focus_in_zone (Super+PgUp/PgDn)"
```

---

## Task 21: D-Bus wire types

**Files:**
- Create: `crates/gnome-zones-daemon/src/dbus/mod.rs`
- Create: `crates/gnome-zones-daemon/src/dbus/types.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Create `src/dbus/types.rs`**

```rust
// crates/gnome-zones-daemon/src/dbus/types.rs
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Wire type for a zone. D-Bus signature: (uddddd).
///
/// Kept separate from crate::model::ZoneRect so that internal Rust types
/// can evolve without breaking the D-Bus ABI.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ZoneWire {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<&crate::model::ZoneRect> for ZoneWire {
    fn from(r: &crate::model::ZoneRect) -> Self {
        Self { zone_index: r.zone_index, x: r.x, y: r.y, w: r.w, h: r.h }
    }
}

impl From<ZoneWire> for crate::model::ZoneRect {
    fn from(w: ZoneWire) -> Self {
        Self { zone_index: w.zone_index, x: w.x, y: w.y, w: w.w, h: w.h }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutSummaryWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

impl From<&crate::model::LayoutSummary> for LayoutSummaryWire {
    fn from(s: &crate::model::LayoutSummary) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            is_preset: s.is_preset,
            zone_count: s.zone_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneWire>,
}

impl From<&crate::model::Layout> for LayoutWire {
    fn from(l: &crate::model::Layout) -> Self {
        Self {
            id: l.id,
            name: l.name.clone(),
            is_preset: l.is_preset,
            zones: l.zones.iter().map(ZoneWire::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MonitorInfoWire {
    pub monitor_key: String,
    pub connector: String,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub is_primary: bool,
}

impl From<&crate::model::MonitorInfo> for MonitorInfoWire {
    fn from(m: &crate::model::MonitorInfo) -> Self {
        Self {
            monitor_key: m.monitor_key.clone(),
            connector: m.connector.clone(),
            name: m.name.clone(),
            width_px: m.width_px,
            height_px: m.height_px,
            is_primary: m.is_primary,
        }
    }
}
```

- [ ] **Step 2: Create stub `src/dbus/mod.rs`**

```rust
// crates/gnome-zones-daemon/src/dbus/mod.rs
pub mod types;
// interface + run_service implemented in Task 22.
```

- [ ] **Step 3: Declare module**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod dbus;
mod error;
mod math;
mod model;
mod monitors;
mod presets;
mod snap;
mod window;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p gnome-zones-daemon
```

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): D-Bus wire types"
```

---

## Task 22: D-Bus `ZonesInterface`

**Files:**
- Create: `crates/gnome-zones-daemon/src/dbus/interface.rs`
- Modify: `crates/gnome-zones-daemon/src/dbus/mod.rs`

- [ ] **Step 1: Create `src/dbus/interface.rs`**

```rust
// crates/gnome-zones-daemon/src/dbus/interface.rs
use crate::db::{layouts, monitors as db_monitors, settings, Database};
use crate::dbus::types::*;
use crate::error::Error;
use crate::model::{IterateDir, ZoneRect};
use crate::monitors::MonitorService;
use crate::snap::SnapEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::{fdo, interface, SignalContext};

pub struct ZonesInterface {
    pub db: Arc<Mutex<Database>>,
    pub snap: Arc<SnapEngine>,
    pub monitor_svc: Arc<dyn MonitorService>,
}

fn fdo_error(e: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

#[interface(name = "org.gnome.Zones")]
impl ZonesInterface {
    // ---- Layout methods ----

    async fn list_layouts(&self) -> fdo::Result<Vec<LayoutSummaryWire>> {
        let db = self.db.lock().await;
        let v = layouts::list_layouts(&db).map_err(fdo_error)?;
        Ok(v.iter().map(LayoutSummaryWire::from).collect())
    }

    async fn get_layout(&self, id: i64) -> fdo::Result<LayoutWire> {
        let db = self.db.lock().await;
        let layout = layouts::get_layout(&db, id)
            .map_err(fdo_error)?
            .ok_or_else(|| fdo::Error::Failed(format!("no layout id={id}")))?;
        Ok(LayoutWire::from(&layout))
    }

    async fn create_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        name: &str,
        zones: Vec<ZoneWire>,
    ) -> fdo::Result<i64> {
        let rects: Vec<ZoneRect> = zones.into_iter().map(Into::into).collect();
        let id = {
            let mut db = self.db.lock().await;
            layouts::create_layout(&mut db, name, false, &rects).map_err(fdo_error)?
        };
        Self::layouts_changed(&ctx).await.ok();
        Ok(id)
    }

    async fn update_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        id: i64,
        name: &str,
        zones: Vec<ZoneWire>,
    ) -> fdo::Result<()> {
        let rects: Vec<ZoneRect> = zones.into_iter().map(Into::into).collect();
        {
            let mut db = self.db.lock().await;
            layouts::update_layout(&mut db, id, name, &rects).map_err(fdo_error)?;
        }
        Self::layouts_changed(&ctx).await.ok();
        Ok(())
    }

    async fn delete_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        id: i64,
    ) -> fdo::Result<()> {
        {
            let mut db = self.db.lock().await;
            layouts::delete_layout(&mut db, id).map_err(fdo_error)?;
        }
        Self::layouts_changed(&ctx).await.ok();
        Ok(())
    }

    // ---- Monitor methods ----

    async fn list_monitors(&self) -> fdo::Result<Vec<MonitorInfoWire>> {
        let v = self.monitor_svc.list_monitors().await.map_err(fdo_error)?;
        Ok(v.iter().map(MonitorInfoWire::from).collect())
    }

    async fn assign_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        monitor_key: &str,
        layout_id: i64,
    ) -> fdo::Result<()> {
        {
            let db = self.db.lock().await;
            db_monitors::assign_layout(&db, monitor_key, layout_id).map_err(fdo_error)?;
        }
        Self::layout_assigned(&ctx, monitor_key.to_string(), layout_id).await.ok();
        Ok(())
    }

    async fn get_active_layout(&self, monitor_key: &str) -> fdo::Result<LayoutWire> {
        let layout = self.snap.active_layout_for(monitor_key).await.map_err(fdo_error)?;
        Ok(LayoutWire::from(&layout))
    }

    // ---- Settings methods ----

    async fn get_settings(&self) -> fdo::Result<HashMap<String, String>> {
        let db = self.db.lock().await;
        settings::get_all_settings(&db).map_err(fdo_error)
    }

    async fn set_setting(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        key: &str,
        value: &str,
    ) -> fdo::Result<()> {
        {
            let db = self.db.lock().await;
            settings::set_setting(&db, key, value).map_err(fdo_error)?;
        }
        if key == "paused" {
            let paused = value == "1" || value == "true";
            Self::paused_changed(&ctx, paused).await.ok();
        }
        Ok(())
    }

    // ---- Action methods ----

    async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> fdo::Result<()> {
        self.snap.snap_focused_to_zone(zone_index, span).await.map_err(fdo_error)
    }

    async fn iterate_focused_zone(&self, direction: &str) -> fdo::Result<()> {
        let dir: IterateDir = direction.parse().map_err(|e: String| fdo::Error::Failed(e))?;
        self.snap.iterate_focused_zone(dir).await.map_err(fdo_error)
    }

    async fn cycle_focus_in_zone(&self, direction: i32) -> fdo::Result<()> {
        self.snap.cycle_focus_in_zone(direction).await.map_err(fdo_error)
    }

    async fn show_activator(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> fdo::Result<()> {
        // UI isn't implemented yet (Plan 2). Emit the signal anyway so the
        // wire surface is complete.
        let primary_key = self
            .monitor_svc.list_monitors().await.map_err(fdo_error)?
            .into_iter()
            .find(|m| m.is_primary)
            .map(|m| m.monitor_key)
            .unwrap_or_default();
        Self::activator_requested(&ctx, primary_key).await.ok();
        Ok(())
    }

    async fn toggle_paused(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> fdo::Result<()> {
        let new_value = {
            let db = self.db.lock().await;
            let current = settings::get_bool(&db, "paused", false).map_err(fdo_error)?;
            let next = !current;
            settings::set_setting(&db, "paused", if next { "true" } else { "false" })
                .map_err(fdo_error)?;
            next
        };
        Self::paused_changed(&ctx, new_value).await.ok();
        Ok(())
    }

    // ---- Signals ----

    #[zbus(signal)]
    async fn layouts_changed(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layout_assigned(ctx: &SignalContext<'_>, monitor_key: String, layout_id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn monitors_changed(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paused_changed(ctx: &SignalContext<'_>, paused: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activator_requested(ctx: &SignalContext<'_>, monitor_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn editor_requested(ctx: &SignalContext<'_>, monitor_key: String) -> zbus::Result<()>;
}
```

- [ ] **Step 2: Wire `run_service` into `src/dbus/mod.rs`**

```rust
// crates/gnome-zones-daemon/src/dbus/mod.rs
pub mod interface;
pub mod types;

use crate::db::Database;
use crate::error::Result;
use crate::monitors::MonitorService;
use crate::snap::SnapEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::ConnectionBuilder;

pub struct ServiceHandle {
    pub connection: zbus::Connection,
}

pub async fn run_service(
    db: Arc<Mutex<Database>>,
    snap: Arc<SnapEngine>,
    monitor_svc: Arc<dyn MonitorService>,
) -> Result<ServiceHandle> {
    let iface = interface::ZonesInterface {
        db: db.clone(),
        snap: snap.clone(),
        monitor_svc: monitor_svc.clone(),
    };
    let connection = ConnectionBuilder::session()?
        .name("org.gnome.Zones")?
        .serve_at("/org/gnome/Zones", iface)?
        .build()
        .await?;
    Ok(ServiceHandle { connection })
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p gnome-zones-daemon
```

Expected: compiles. (D-Bus interfaces are hard to unit-test without a live bus; we'll smoke-test in the manual checklist at the end of the plan.)

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src/dbus
git commit -m "feat(zones-daemon): D-Bus ZonesInterface (org.gnome.Zones)"
```

---

## Task 23: Hotkey registration helpers

**Files:**
- Create: `crates/gnome-zones-daemon/src/hotkeys.rs`
- Modify: `crates/gnome-zones-daemon/src/main.rs`

GNOME custom keybindings live in `org.gnome.settings-daemon.plugins.media-keys.custom-keybindings` as an array of object paths. Each path points to a `custom-keybinding` schema instance with `name`, `command`, and `binding` keys. We:

1. Stash GNOME's existing `toggle-tiled-left` and `toggle-tiled-right` in our `settings` table (spec §4 "Conflict with GNOME defaults").
2. Unbind them.
3. Register our 14 shortcuts as custom keybindings. Each fires a `busctl --user` command.

All interaction with gsettings happens via the `gsettings` CLI (fork/exec). zbus-based `gsettings` over the session bus would be nicer but requires schema introspection that's out of scope for this task.

- [ ] **Step 1: Create `src/hotkeys.rs`**

```rust
// crates/gnome-zones-daemon/src/hotkeys.rs
use crate::db::{settings, Database};
use crate::error::{Error, Result};
use std::process::Command;

const KEYBIND_PREFIX: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnome-zones";
const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_BINDING_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const MUTTER_KB_SCHEMA: &str = "org.gnome.mutter.keybindings";

/// (schema-relative slug, display name, accelerator, busctl command)
///
/// The slugs become `/.../gnome-zones-<slug>/` object paths.
pub fn default_bindings() -> Vec<(&'static str, &'static str, &'static str, String)> {
    let busctl = |method: &str, args: &str| -> String {
        format!(
            "busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones {method} {args}"
        )
    };
    let mut out = Vec::new();
    for n in 1..=9 {
        out.push((
            Box::leak(format!("snap-{n}").into_boxed_str()) as &str,
            Box::leak(format!("Snap to zone {n}").into_boxed_str()) as &str,
            Box::leak(format!("<Super><Control>{n}").into_boxed_str()) as &str,
            busctl("SnapFocusedToZone", &format!("ub {n} false")),
        ));
        out.push((
            Box::leak(format!("span-{n}").into_boxed_str()) as &str,
            Box::leak(format!("Span into zone {n}").into_boxed_str()) as &str,
            Box::leak(format!("<Super><Alt>{n}").into_boxed_str()) as &str,
            busctl("SnapFocusedToZone", &format!("ub {n} true")),
        ));
    }
    out.push(("activator", "Show zone activator", "<Super>grave",
             busctl("ShowActivator", "")));
    out.push(("iter-prev", "Iterate to previous zone", "<Super>Left",
             busctl("IterateFocusedZone", "s prev")));
    out.push(("iter-next", "Iterate to next zone", "<Super>Right",
             busctl("IterateFocusedZone", "s next")));
    out.push(("cycle-prev", "Cycle focus back in zone", "<Super>Page_Up",
             busctl("CycleFocusInZone", "i -1")));
    out.push(("cycle-next", "Cycle focus forward in zone", "<Super>Page_Down",
             busctl("CycleFocusInZone", "i 1")));
    out.push(("editor", "Open zone editor", "<Super><Shift>e",
             busctl("ShowEditor", "")));  // placeholder — Plan 2 adds ShowEditor
    out.push(("pause", "Toggle pause", "<Super><Shift>p",
             busctl("TogglePaused", "")));
    out
}

fn run(cmd: &mut Command) -> Result<String> {
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(Error::Config(format!(
            "gsettings exited {:?}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn gsettings_get(schema: &str, key: &str) -> Result<String> {
    run(Command::new("gsettings").args(["get", schema, key]))
}

fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<()> {
    run(Command::new("gsettings").args(["set", schema, key, value]))?;
    Ok(())
}

fn gsettings_set_with_path(schema: &str, path: &str, key: &str, value: &str) -> Result<()> {
    run(Command::new("gsettings")
        .args(["set", &format!("{schema}:{path}"), key, value]))?;
    Ok(())
}

/// Stash GNOME's default `Super+Left/Right` bindings and disable them.
/// Idempotent — calling again is a no-op once stashed.
pub fn stash_gnome_defaults(db: &Database) -> Result<()> {
    for (gkey, our_key) in [
        ("toggle-tiled-left",  "gnome_default_tile_left"),
        ("toggle-tiled-right", "gnome_default_tile_right"),
    ] {
        if settings::get_setting(db, our_key)?.is_some() {
            continue;  // already stashed on a previous run
        }
        let current = gsettings_get(MUTTER_KB_SCHEMA, gkey)?;
        settings::set_setting(db, our_key, &current)?;
        gsettings_set(MUTTER_KB_SCHEMA, gkey, "[]")?;
    }
    Ok(())
}

/// Restore the stashed GNOME defaults. Called on uninstall or by the user.
pub fn restore_gnome_defaults(db: &Database) -> Result<()> {
    for (gkey, our_key) in [
        ("toggle-tiled-left",  "gnome_default_tile_left"),
        ("toggle-tiled-right", "gnome_default_tile_right"),
    ] {
        if let Some(stashed) = settings::get_setting(db, our_key)? {
            gsettings_set(MUTTER_KB_SCHEMA, gkey, &stashed)?;
        }
    }
    Ok(())
}

/// Register all of our custom keybindings via gsettings. Idempotent.
pub fn register_custom_bindings() -> Result<()> {
    let bindings = default_bindings();
    let mut paths: Vec<String> = Vec::with_capacity(bindings.len());
    for (slug, name, accel, command) in &bindings {
        let path = format!("{KEYBIND_PREFIX}-{slug}/");
        paths.push(path.clone());
        gsettings_set_with_path(CUSTOM_BINDING_SCHEMA, &path, "name", &format!("'{name}'"))?;
        gsettings_set_with_path(CUSTOM_BINDING_SCHEMA, &path, "command", &format!("'{}'", command.replace('\'', "\\'")))?;
        gsettings_set_with_path(CUSTOM_BINDING_SCHEMA, &path, "binding", &format!("'{accel}'"))?;
    }
    // Register all paths in the media-keys custom-keybindings array.
    let array_value = format!(
        "[{}]",
        paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")
    );
    gsettings_set(MEDIA_KEYS_SCHEMA, "custom-keybindings", &array_value)?;
    Ok(())
}

/// Remove our custom keybindings entirely. Called on uninstall.
pub fn unregister_custom_bindings() -> Result<()> {
    gsettings_set(MEDIA_KEYS_SCHEMA, "custom-keybindings", "[]")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_includes_nine_snap_plus_nine_span() {
        let b = default_bindings();
        assert_eq!(b.iter().filter(|(slug, _, _, _)| slug.starts_with("snap-")).count(), 9);
        assert_eq!(b.iter().filter(|(slug, _, _, _)| slug.starts_with("span-")).count(), 9);
    }

    #[test]
    fn default_bindings_has_all_navigation_entries() {
        let b = default_bindings();
        let slugs: Vec<&str> = b.iter().map(|(s, _, _, _)| *s).collect();
        for expected in &["activator", "iter-prev", "iter-next", "cycle-prev", "cycle-next", "editor", "pause"] {
            assert!(slugs.contains(expected), "missing binding: {expected}");
        }
    }

    #[test]
    fn snap_binding_uses_super_ctrl_chord() {
        let b = default_bindings();
        let (_, _, accel, _) = b.iter().find(|(s, _, _, _)| *s == "snap-1").unwrap();
        assert_eq!(*accel, "<Super><Control>1");
    }

    #[test]
    fn span_binding_uses_super_alt_chord() {
        let b = default_bindings();
        let (_, _, accel, _) = b.iter().find(|(s, _, _, _)| *s == "span-1").unwrap();
        assert_eq!(*accel, "<Super><Alt>1");
    }
}
```

- [ ] **Step 2: Declare module**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod dbus;
mod error;
mod hotkeys;
mod math;
mod model;
mod monitors;
mod presets;
mod snap;
mod window;

fn main() {
    println!("gnome-zones-daemon stub");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p gnome-zones-daemon hotkeys
```

Expected: 4 passed. (We don't execute `gsettings` in unit tests — the stash/register functions are exercised manually.)

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): hotkey stash/register via gsettings CLI"
```

---

## Task 24: Monitor hot-plug subscription

**Files:**
- Modify: `crates/gnome-zones-daemon/src/monitors.rs`

Subscribe to the `MonitorsChanged` signal on `org.gnome.Mutter.DisplayConfig`. On each event, auto-assign the "Two Columns (50/50)" preset to any newly-seen monitor that lacks an assignment. The watcher just reconciles the DB — our own D-Bus `MonitorsChanged` signal is emitted from the `ZonesInterface` where the signal context is available (added in Task 24 Step 2 below).

- [ ] **Step 1: Add the hot-plug subscriber to `monitors.rs`**

Append to `src/monitors.rs`:

```rust
use crate::db::{monitors as db_monitors, Database};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use zbus::{Connection, MatchRule, MessageStream, MessageType};
use futures_util::StreamExt;

/// Watch for monitor reconfigurations. Auto-assigns the default layout to
/// newly-seen monitors and fires `notify_tx` on each reconcile so the D-Bus
/// interface can emit the user-facing `MonitorsChanged` signal.
pub async fn spawn_hotplug_watcher(
    conn: Connection,
    db: Arc<Mutex<Database>>,
    monitor_svc: Arc<dyn MonitorService>,
    notify_tx: mpsc::UnboundedSender<()>,
) -> Result<tokio::task::JoinHandle<()>> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface("org.gnome.Mutter.DisplayConfig")?
        .member("MonitorsChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;
    dbus_proxy.add_match_rule(rule).await?;

    let mut stream = MessageStream::from(&conn);
    let handle = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let hdr = msg.header();
            let Ok(Some(member)) = hdr.member() else { continue };
            if member.as_str() != "MonitorsChanged" { continue }

            if let Err(e) = reconcile_monitors(&db, &monitor_svc).await {
                tracing::warn!("monitor reconcile failed: {e}");
                continue;
            }
            // Best-effort — if nobody's listening, just drop the event.
            let _ = notify_tx.send(());
        }
    });
    Ok(handle)
}

async fn reconcile_monitors(
    db: &Arc<Mutex<Database>>,
    monitor_svc: &Arc<dyn MonitorService>,
) -> Result<()> {
    let monitors = monitor_svc.list_monitors().await?;
    let db = db.lock().await;
    let default_id: i64 = db.conn.query_row(
        "SELECT id FROM layouts WHERE name = 'Two Columns (50/50)' AND is_preset = 1",
        [],
        |r| r.get(0),
    )?;
    for m in monitors {
        if db_monitors::get_assigned_layout_id(&db, &m.monitor_key)?.is_none() {
            db_monitors::assign_layout(&db, &m.monitor_key, default_id)?;
            tracing::info!(monitor_key = %m.monitor_key, "new monitor → assigned default layout");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Add a signal-forwarder in `main.rs` wiring**

The forwarder runs inside `main` (Task 25) and uses the `SignalContext` from the D-Bus connection to emit `org.gnome.Zones.MonitorsChanged` each time `notify_rx` fires. We add an internal method on `ZonesInterface` that uses its own emit machinery — concretely we expose a public helper on `ServiceHandle` that looks up the interface reference and calls the signal.

Add to `src/dbus/mod.rs`:

```rust
use crate::dbus::interface::ZonesInterface;

impl ServiceHandle {
    /// Emit `org.gnome.Zones.MonitorsChanged` from outside the interface.
    /// Used by the hot-plug watcher.
    pub async fn emit_monitors_changed(&self) -> Result<()> {
        let iface_ref: zbus::InterfaceRef<ZonesInterface> = self
            .connection
            .object_server()
            .interface("/org/gnome/Zones")
            .await?;
        ZonesInterface::monitors_changed(iface_ref.signal_context()).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p gnome-zones-daemon
```

No new tests — this requires a live compositor. It's exercised by the manual smoke test in Task 27.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src
git commit -m "feat(zones-daemon): monitor hot-plug reconciliation + MonitorsChanged signal"
```

---

## Task 25: `main.rs` — wire everything

**Files:**
- Modify: `crates/gnome-zones-daemon/src/main.rs`

- [ ] **Step 1: Implement the real main**

```rust
// crates/gnome-zones-daemon/src/main.rs
mod db;
mod dbus;
mod error;
mod hotkeys;
mod math;
mod model;
mod monitors;
mod presets;
mod snap;
mod window;

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::db::Database;
use crate::monitors::MutterMonitorService;
use crate::snap::state::WindowStateMap;
use crate::snap::SnapEngine;
use crate::window::shim::ShimMover;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> crate::error::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,gnome_zones_daemon=debug")))
        .init();

    tracing::info!("gnome-zones-daemon starting");

    // --- Persistence ---
    let mut db = Database::open_default()?;
    presets::seed(&mut db)?;
    let db = Arc::new(Mutex::new(db));

    // --- D-Bus session connection ---
    let session_conn = zbus::Connection::session().await?;

    // --- Services ---
    let monitor_svc: Arc<dyn monitors::MonitorService> =
        Arc::new(MutterMonitorService::new(&session_conn).await?);

    let mover: Arc<dyn window::WindowMover> =
        match ShimMover::new(&session_conn).await {
            Ok(m) => {
                tracing::info!("window mover: gnome-zones-mover shell extension");
                Arc::new(m)
            }
            Err(e) => {
                tracing::warn!("shim unavailable ({e}); falling back to MutterMover");
                Arc::new(window::mutter::MutterMover::new(&session_conn).await?)
            }
        };

    // --- Snap engine ---
    let states = Arc::new(WindowStateMap::new());
    let snap_engine = Arc::new(SnapEngine::new(
        db.clone(),
        monitor_svc.clone(),
        mover.clone(),
        states.clone(),
    ));

    // --- First-run hotkey registration ---
    {
        let db_guard = db.lock().await;
        if let Err(e) = hotkeys::stash_gnome_defaults(&db_guard) {
            tracing::warn!("could not stash GNOME defaults: {e}");
        }
    }
    if let Err(e) = hotkeys::register_custom_bindings() {
        tracing::warn!("could not register hotkeys: {e}");
    }

    // --- D-Bus service ---
    let service = Arc::new(
        dbus::run_service(db.clone(), snap_engine.clone(), monitor_svc.clone()).await?
    );

    // --- Monitor hot-plug watcher ---
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel();
    let _hotplug = monitors::spawn_hotplug_watcher(
        session_conn.clone(),
        db.clone(),
        monitor_svc.clone(),
        notify_tx,
    ).await?;

    // Forward reconciliation notifications to our user-facing D-Bus signal.
    {
        let service = service.clone();
        tokio::spawn(async move {
            while notify_rx.recv().await.is_some() {
                if let Err(e) = service.emit_monitors_changed().await {
                    tracing::warn!("MonitorsChanged emit failed: {e}");
                }
            }
        });
    }

    tracing::info!("gnome-zones-daemon ready");

    // Park the main task until shutdown. `service` is kept alive through the
    // spawned task's Arc clone AND this local binding below.
    let _keep_service = service;
    tokio::signal::ctrl_c().await?;
    tracing::info!("gnome-zones-daemon shutting down");
    Ok(())
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p gnome-zones-daemon
```

Expected: compiles.

- [ ] **Step 3: Run unit tests one more time end-to-end**

```bash
cargo test -p gnome-zones-daemon
```

Expected: all tests pass (approximately 40+ tests total across all modules).

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones-daemon/src/main.rs
git commit -m "feat(zones-daemon): wire tokio runtime, D-Bus service, hot-plug watcher"
```

---

## Task 26: systemd user unit

**Files:**
- Create: `dist/systemd/gnome-zones-daemon.service`

- [ ] **Step 1: Create the unit file**

```ini
# dist/systemd/gnome-zones-daemon.service
[Unit]
Description=gnome-zones daemon — keyboard-driven window zones
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/gnome-zones-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=graphical-session.target
```

Packaging (Plan 3) installs this to `/usr/lib/systemd/user/`.

- [ ] **Step 2: Commit**

```bash
git add dist/systemd/gnome-zones-daemon.service
git commit -m "feat(zones): systemd user unit"
```

---

## Task 27: Manual smoke-test checklist

**Files:**
- Create: `dist/test/zones-manual.md`

- [ ] **Step 1: Create the checklist**

```markdown
# gnome-zones Manual Smoke Test

Run on a real GNOME session. These test paths no unit test can cover.

## Prereqs

1. Install `gnome-zones-mover@power-toys`:
   ```bash
   mkdir -p ~/.local/share/gnome-shell/extensions/
   cp -r dist/shell-extension/gnome-zones-mover@power-toys/ \
         ~/.local/share/gnome-shell/extensions/
   ```
   Log out and back in (X11) or press `Alt+F2` `r` `Enter` (X11 only — on Wayland you must log out).
   Then enable:
   ```bash
   gnome-extensions enable gnome-zones-mover@power-toys
   ```

2. Build and run the daemon:
   ```bash
   cargo run -p gnome-zones-daemon
   ```

## Tests

### T1: D-Bus surface
```bash
busctl --user introspect org.gnome.Zones /org/gnome/Zones
```
Expect: `org.gnome.Zones` interface listed with every method from the spec.

### T2: Preset seeding
```bash
busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones ListLayouts
```
Expect: 8 preset layouts listed.

### T3: Snap to zone 1
Focus any window. Press `Super+Ctrl+1`. Expect: window resizes to cover the left half.

### T4: Snap to zone 2
Press `Super+Ctrl+2`. Expect: window resizes to cover the right half.

### T5: Iterate
Snap to zone 1, then press `Super+Right`. Expect: window moves to zone 2. Press again. Expect: wraps back to zone 1.

### T6: Activator
Press `` Super+` ``. Expect: `ActivatorRequested` signal is emitted (verify with `dbus-monitor`). No visual UI yet — that's Plan 2.

### T7: Span
Snap to zone 1, then press `Super+Alt+2`. Expect: window spans both halves (full width minus gap).

### T8: Pause
Press `Super+Shift+P`. Try `Super+Ctrl+1`. Expect: no movement (paused). Press `Super+Shift+P` again. Expect: shortcuts work again.

### T9: Restore defaults
Stop the daemon. Run:
```bash
busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones SetSetting ss "paused" "true"
```
(Set any value to verify the DB path works.) Kill the daemon. Check that `~/.local/share/gnome-zones/zones.db` exists.
```

- [ ] **Step 2: Commit**

```bash
git add dist/test/zones-manual.md
git commit -m "docs(zones): manual smoke-test checklist"
```

---

## Summary

After all 27 tasks, you have a working daemon that:

1. Seeds 8 preset layouts into a local SQLite DB on first run
2. Enumerates monitors via `org.gnome.Mutter.DisplayConfig` and auto-assigns the default layout
3. Registers global hotkeys via GNOME's custom-keybindings system (stashing the stock `Super+Left/Right` bindings first)
4. Exposes the full `org.gnome.Zones` D-Bus interface
5. Snaps, spans, iterates, and cycles window focus when hotkeys fire
6. Uses the shell-extension shim (with a Mutter-introspect fallback) to move windows

Plans 2 and 3 build on this — Plan 2 adds the UI overlays and panel icon; Plan 3 wraps everything in Debian and Flatpak packages.
