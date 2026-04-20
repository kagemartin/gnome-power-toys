use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("D-Bus fdo error: {0}")]
    Fdo(#[from] zbus::fdo::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fdo_error_converts_to_ours() {
        let e: Error = zbus::fdo::Error::Failed("boom".into()).into();
        let s = format!("{e}");
        assert!(s.contains("fdo"));
        assert!(s.contains("boom"));
    }
}
