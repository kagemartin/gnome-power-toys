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
