use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("D-Bus fdo error: {0}")]
    Fdo(#[from] zbus::fdo::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
