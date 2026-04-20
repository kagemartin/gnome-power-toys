//! Write a clip's content onto the system clipboard, then close the
//! popup. The user then pastes in whichever app they want with Ctrl+V —
//! the same model every GNOME clipboard-history tool uses.

use std::borrow::Cow;

use gtk4::gdk;
use gtk4::gdk::prelude::*;
use gtk4::glib;

/// Pure description of how a stored clip should land on the system
/// clipboard. Separated from the IO for testability.
#[derive(Debug, PartialEq)]
pub enum ClipboardPayload<'a> {
    /// Textual payload — write via `gdk::Clipboard::set_text` so any app
    /// that wants `text/plain` can read it regardless of the stored MIME.
    Text(Cow<'a, str>),
    /// Raster image — decode and set as texture (bytes are the full
    /// image file, format-detected by GDK).
    Texture(&'a [u8]),
    /// File reference — bytes are a filesystem path; land on the
    /// clipboard as a `text/uri-list`.
    FileUri(String),
    /// Passthrough: set the raw bytes under the stored MIME.
    Raw { mime: String, bytes: Vec<u8> },
    /// We stored something we can't represent (e.g. text declared as
    /// text/plain but not valid UTF-8). Treat as raw.
    Unsupported,
}

pub fn payload_for<'a>(content_type: &'a str, data: &'a [u8]) -> ClipboardPayload<'a> {
    match content_type {
        "text/plain" | "text/html" | "text/markdown" => match std::str::from_utf8(data) {
            Ok(s) => ClipboardPayload::Text(Cow::Borrowed(s)),
            Err(_) => ClipboardPayload::Unsupported,
        },
        t if t.starts_with("image/") => ClipboardPayload::Texture(data),
        "application/file" => match std::str::from_utf8(data) {
            Ok(path) => {
                let path = path.trim();
                let uri = if path.starts_with("file://") {
                    path.to_string()
                } else {
                    format!("file://{path}")
                };
                ClipboardPayload::FileUri(uri)
            }
            Err(_) => ClipboardPayload::Unsupported,
        },
        other => ClipboardPayload::Raw {
            mime: other.to_string(),
            bytes: data.to_vec(),
        },
    }
}

/// IO side: apply a payload to the given clipboard.
pub fn apply(clipboard: &gdk::Clipboard, payload: ClipboardPayload<'_>) {
    match payload {
        ClipboardPayload::Text(s) => clipboard.set_text(&s),
        ClipboardPayload::Texture(bytes) => {
            let gbytes = glib::Bytes::from(bytes);
            match gdk::Texture::from_bytes(&gbytes) {
                Ok(tex) => clipboard.set_texture(&tex),
                Err(e) => tracing::warn!(%e, "failed to decode image for clipboard"),
            }
        }
        ClipboardPayload::FileUri(uri) => {
            let bytes = glib::Bytes::from_owned(uri.into_bytes());
            let provider = gdk::ContentProvider::for_bytes("text/uri-list", &bytes);
            if let Err(e) = clipboard.set_content(Some(&provider)) {
                tracing::warn!(%e, "failed to set text/uri-list on clipboard");
            }
        }
        ClipboardPayload::Raw { mime, bytes } => {
            let gbytes = glib::Bytes::from_owned(bytes);
            let provider = gdk::ContentProvider::for_bytes(&mime, &gbytes);
            if let Err(e) = clipboard.set_content(Some(&provider)) {
                tracing::warn!(%e, mime = %mime, "failed to set custom MIME on clipboard");
            }
        }
        ClipboardPayload::Unsupported => {
            tracing::warn!("clip content could not be represented on the clipboard");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_plain_utf8_becomes_text() {
        let p = payload_for("text/plain", b"hello");
        assert_eq!(p, ClipboardPayload::Text(Cow::Borrowed("hello")));
    }

    #[test]
    fn text_html_becomes_text() {
        let p = payload_for("text/html", b"<b>x</b>");
        assert_eq!(p, ClipboardPayload::Text(Cow::Borrowed("<b>x</b>")));
    }

    #[test]
    fn text_markdown_becomes_text() {
        let p = payload_for("text/markdown", b"# title");
        assert_eq!(p, ClipboardPayload::Text(Cow::Borrowed("# title")));
    }

    #[test]
    fn text_with_invalid_utf8_is_unsupported() {
        let p = payload_for("text/plain", &[0xff, 0xfe, 0xfd]);
        assert_eq!(p, ClipboardPayload::Unsupported);
    }

    #[test]
    fn image_becomes_texture() {
        let bytes = [0x89u8, 0x50, 0x4e, 0x47];
        match payload_for("image/png", &bytes) {
            ClipboardPayload::Texture(b) => assert_eq!(b, &bytes),
            other => panic!("expected Texture, got {other:?}"),
        }
    }

    #[test]
    fn image_jpeg_also_becomes_texture() {
        match payload_for("image/jpeg", &[0xff, 0xd8, 0xff]) {
            ClipboardPayload::Texture(_) => {}
            other => panic!("expected Texture, got {other:?}"),
        }
    }

    #[test]
    fn file_path_becomes_uri() {
        let p = payload_for("application/file", b"/home/x/a.txt");
        assert_eq!(p, ClipboardPayload::FileUri("file:///home/x/a.txt".into()));
    }

    #[test]
    fn file_already_uri_is_preserved() {
        let p = payload_for("application/file", b"file:///already/there.txt");
        assert_eq!(
            p,
            ClipboardPayload::FileUri("file:///already/there.txt".into())
        );
    }

    #[test]
    fn file_path_trims_surrounding_whitespace() {
        let p = payload_for("application/file", b"  /p/x  \n");
        assert_eq!(p, ClipboardPayload::FileUri("file:///p/x".into()));
    }

    #[test]
    fn unknown_mime_is_passthrough() {
        let p = payload_for("application/x-custom", &[1, 2, 3]);
        assert_eq!(
            p,
            ClipboardPayload::Raw {
                mime: "application/x-custom".into(),
                bytes: vec![1, 2, 3]
            }
        );
    }
}
