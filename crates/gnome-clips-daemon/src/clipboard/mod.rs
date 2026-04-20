pub mod wayland;
pub mod x11;

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

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
