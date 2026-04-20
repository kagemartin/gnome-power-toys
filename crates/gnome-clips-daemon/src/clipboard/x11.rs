//! X11 clipboard monitor using x11-clipboard crate.
//! Polls every 500ms as Wayland monitor does.

use std::time::Duration;
use tokio::sync::mpsc;
use x11_clipboard::Clipboard;

use super::{content_hash, ClipboardEvent, ContentHash};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub async fn poll_x11(tx: mpsc::Sender<ClipboardEvent>) {
    let clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => {
            tracing::error!("failed to open X11 clipboard");
            return;
        }
    };

    let mut last_hash: Option<ContentHash> = None;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let content = clipboard.load(
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            Duration::from_millis(100),
        );

        if let Ok(content) = content {
            if content.is_empty() {
                continue;
            }
            let hash = content_hash(&content);
            if Some(hash) != last_hash {
                last_hash = Some(hash);
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
