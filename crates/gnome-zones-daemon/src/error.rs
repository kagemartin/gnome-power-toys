use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("D-Bus zvariant error: {0}")]
    ZVariant(#[from] zbus::zvariant::Error),

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
