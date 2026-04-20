pub mod wayland;
pub mod writer;
pub mod x11;

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex};

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

/// Backend-specific handle to the system clipboard, cloneable into both the
/// polling task and the writer used by D-Bus `Paste`.
///
/// Keeping a single `x11_clipboard::Clipboard` shared between reader and
/// writer is deliberate: that crate spawns a helper thread that *serves*
/// selection requests for the CLIPBOARD selection until the Clipboard is
/// dropped. If the UI writes the clipboard directly, ownership dies as
/// soon as the popup hides; moving writes into the daemon means ownership
/// lives as long as the daemon does.
#[derive(Clone)]
pub enum Backend {
    X11(Arc<Mutex<x11_clipboard::Clipboard>>),
    Wayland,
    /// For unit tests that don't have a display.
    Noop,
}

impl Backend {
    pub fn detect() -> Result<Self, writer::Error> {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            Ok(Backend::Wayland)
        } else {
            let clip = x11_clipboard::Clipboard::new()
                .map_err(|e| writer::Error::Init(format!("x11 clipboard init: {e:?}")))?;
            Ok(Backend::X11(Arc::new(Mutex::new(clip))))
        }
    }
}

/// Start the clipboard-monitoring task for this session. The shared
/// `last_hash` is what lets the writer's paste-back not be re-stored as a
/// new clip (the poll sees the matching hash and skips it).
pub async fn start_monitor(
    backend: Backend,
    last_hash: Arc<Mutex<Option<ContentHash>>>,
    tx: mpsc::Sender<ClipboardEvent>,
) {
    match backend {
        Backend::Wayland => {
            tokio::spawn(wayland::poll_wayland(last_hash, tx));
        }
        Backend::X11(clip) => {
            tokio::spawn(x11::poll_x11(clip, last_hash, tx));
        }
        Backend::Noop => {}
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
