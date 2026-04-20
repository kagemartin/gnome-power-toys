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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_error_displays() {
        let e = Error::Db(rusqlite::Error::InvalidColumnName("x".into()));
        assert!(e.to_string().contains("database"));
    }
}
