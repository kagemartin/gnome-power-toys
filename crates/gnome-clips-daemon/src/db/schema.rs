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
