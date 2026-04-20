//! Polls the Wayland clipboard every 500ms using wl-clipboard-rs.
//! Detects changes by hashing content.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use wl_clipboard_rs::paste::{get_contents, get_mime_types, ClipboardType, Error, MimeType, Seat};

use super::{content_hash, ClipboardEvent, ContentHash};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// MIME type priority — highest priority first.
const MIME_PRIORITY: &[&str] = &[
    "text/html",
    "text/markdown",
    "image/png",
    "image/jpeg",
    "application/octet-stream",
    "text/plain",
];

pub async fn poll_wayland(
    last_hash: Arc<Mutex<Option<ContentHash>>>,
    tx: mpsc::Sender<ClipboardEvent>,
) {
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        if let Some(event) = read_clipboard() {
            let hash = content_hash(&event.content);
            let mut lh = last_hash.lock().await;
            if Some(hash) != *lh {
                *lh = Some(hash);
                drop(lh);
                let _ = tx.send(event).await;
            }
        }
    }
}

fn read_clipboard() -> Option<ClipboardEvent> {
    let types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(t) => t,
        Err(_) => return None,
    };

    let chosen_mime: String = MIME_PRIORITY
        .iter()
        .find(|&&m| types.contains(m))
        .map(|m| m.to_string())
        .or_else(|| types.iter().find(|t| t.starts_with("text/")).cloned())?;

    let result = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(&chosen_mime),
    );

    match result {
        Ok((mut reader, _)) => {
            let mut content = Vec::new();
            if reader.read_to_end(&mut content).is_err() || content.is_empty() {
                return None;
            }
            let content_type = if chosen_mime == "application/octet-stream" {
                "application/file".to_string()
            } else {
                chosen_mime
            };
            Some(ClipboardEvent {
                content,
                content_type,
                source_app: None,
            })
        }
        Err(Error::NoSeats) | Err(Error::ClipboardEmpty) | Err(Error::NoMimeType) => None,
        Err(_) => None,
    }
}
