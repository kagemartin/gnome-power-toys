//! X11 clipboard monitor using x11-clipboard crate.
//! Polls every 500ms as the Wayland monitor does.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use x11_clipboard::Clipboard;

use super::{content_hash, ClipboardEvent, ContentHash};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub async fn poll_x11(
    clipboard: Arc<Mutex<Clipboard>>,
    last_hash: Arc<Mutex<Option<ContentHash>>>,
    tx: mpsc::Sender<ClipboardEvent>,
) {
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let content = {
            let clip = clipboard.lock().await;
            clip.load(
                clip.getter.atoms.clipboard,
                clip.getter.atoms.utf8_string,
                clip.getter.atoms.property,
                Duration::from_millis(100),
            )
        };

        if let Ok(content) = content {
            if content.is_empty() {
                continue;
            }
            let hash = content_hash(&content);
            let mut lh = last_hash.lock().await;
            if Some(hash) != *lh {
                *lh = Some(hash);
                drop(lh);
                let _ = tx
                    .send(ClipboardEvent {
                        content,
                        content_type: "text/plain".to_string(),
                        source_app: None,
                    })
                    .await;
            }
        }
    }
}
