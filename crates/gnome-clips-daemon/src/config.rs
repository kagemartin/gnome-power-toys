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
