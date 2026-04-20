//! Writes clips back to the system clipboard. The daemon — not the UI —
//! owns the clipboard selection because it lives for the whole session;
//! the UI popup can be destroyed between pastes without losing content.

use std::sync::Arc;

use tokio::sync::Mutex;
use wl_clipboard_rs::copy::{copy, MimeType as WlMimeType, Options, Source};
use x11_clipboard::Clipboard;

use super::{content_hash, Backend, ContentHash};

#[derive(Debug)]
pub enum Error {
    Init(String),
    X11(String),
    Wayland(String),
    InvalidUtf8,
    NoBackend,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Init(s) => write!(f, "clipboard init: {s}"),
            Error::X11(s) => write!(f, "x11 clipboard: {s}"),
            Error::Wayland(s) => write!(f, "wayland clipboard: {s}"),
            Error::InvalidUtf8 => write!(f, "invalid utf-8 in file-path clip"),
            Error::NoBackend => write!(f, "clipboard writer disabled"),
        }
    }
}

impl std::error::Error for Error {}

pub struct ClipboardWriter {
    backend: Backend,
    last_hash: Arc<Mutex<Option<ContentHash>>>,
}

impl ClipboardWriter {
    pub fn new(backend: Backend, last_hash: Arc<Mutex<Option<ContentHash>>>) -> Self {
        Self { backend, last_hash }
    }

    /// Writes bytes to the system clipboard under the given MIME. Text
    /// variants (`text/plain`, `text/html`, `text/markdown`) collapse to
    /// UTF8_STRING on X11 and MimeType::Text on Wayland so ordinary text
    /// editors can paste them. `application/file` becomes a `text/uri-list`.
    pub async fn write(&self, content: &[u8], content_type: &str) -> Result<(), Error> {
        // Prevent our own poll from re-storing this paste as a new clip.
        {
            let mut lh = self.last_hash.lock().await;
            *lh = Some(content_hash(content));
        }

        match &self.backend {
            Backend::X11(clip) => write_x11(clip, content, content_type).await,
            Backend::Wayland => write_wayland(content, content_type),
            Backend::Noop => Err(Error::NoBackend),
        }
    }
}

async fn write_x11(clip: &Arc<Mutex<Clipboard>>, content: &[u8], mime: &str) -> Result<(), Error> {
    let clip_guard = clip.lock().await;
    let atoms = &clip_guard.setter.atoms;

    let (target_atom, payload) = match mime {
        "text/plain" | "text/html" | "text/markdown" => (atoms.utf8_string, content.to_vec()),
        "application/file" => {
            let uri = to_file_uri(content)?.into_bytes();
            let atom = clip_guard
                .setter
                .get_atom("text/uri-list")
                .map_err(|e| Error::X11(format!("{e:?}")))?;
            (atom, uri)
        }
        other => {
            let atom = clip_guard
                .setter
                .get_atom(other)
                .map_err(|e| Error::X11(format!("{e:?}")))?;
            (atom, content.to_vec())
        }
    };

    // x11_clipboard::Clipboard::store verifies ownership internally
    // before returning Ok, and keeps its worker thread alive to serve
    // SelectionRequests for the daemon's lifetime.
    clip_guard
        .store(atoms.clipboard, target_atom, payload)
        .map_err(|e| Error::X11(format!("{e:?}")))?;
    Ok(())
}

fn write_wayland(content: &[u8], mime: &str) -> Result<(), Error> {
    let mut opts = Options::new();
    // foreground=false makes wl-clipboard-rs spawn a helper thread that
    // serves the data until the clipboard is overwritten.
    opts.foreground(false);

    let (wl_mime, payload) = match mime {
        "text/plain" | "text/html" | "text/markdown" => (WlMimeType::Text, content.to_vec()),
        "application/file" => (
            WlMimeType::Specific("text/uri-list".into()),
            to_file_uri(content)?.into_bytes(),
        ),
        other => (WlMimeType::Specific(other.into()), content.to_vec()),
    };

    copy(opts, Source::Bytes(payload.into_boxed_slice()), wl_mime)
        .map_err(|e| Error::Wayland(format!("{e}")))?;
    Ok(())
}

pub fn to_file_uri(bytes: &[u8]) -> Result<String, Error> {
    let path = std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidUtf8)?
        .trim();
    Ok(if path.starts_with("file://") {
        path.to_string()
    } else {
        format!("file://{path}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_path_gets_file_prefix() {
        assert_eq!(
            to_file_uri(b"/home/x/a.txt").unwrap(),
            "file:///home/x/a.txt"
        );
    }

    #[test]
    fn already_uri_is_preserved() {
        assert_eq!(to_file_uri(b"file:///a.txt").unwrap(), "file:///a.txt");
    }

    #[test]
    fn path_gets_trimmed() {
        assert_eq!(to_file_uri(b"  /x\n").unwrap(), "file:///x");
    }

    #[test]
    fn invalid_utf8_errors() {
        assert!(matches!(to_file_uri(&[0xff, 0xfe]), Err(Error::InvalidUtf8)));
    }

    #[tokio::test]
    async fn noop_backend_reports_no_backend() {
        let lh = Arc::new(Mutex::new(None));
        let w = ClipboardWriter::new(Backend::Noop, lh);
        let r = w.write(b"hi", "text/plain").await;
        assert!(matches!(r, Err(Error::NoBackend)));
    }

    #[tokio::test]
    async fn write_updates_last_hash_even_on_noop_backend() {
        let lh = Arc::new(Mutex::new(None));
        let w = ClipboardWriter::new(Backend::Noop, lh.clone());
        let _ = w.write(b"abc", "text/plain").await;
        assert_eq!(*lh.lock().await, Some(content_hash(b"abc")));
    }
}
