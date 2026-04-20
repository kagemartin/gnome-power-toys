use gtk4::prelude::*;
use gtk4::{Box as GBox, Button, FlowBox, Label, Orientation, ScrolledWindow, TextBuffer, TextView};

use crate::dbus::ClipDetail;

pub struct PreviewPane {
    pub container: GBox,
    pub pin_btn: Button,
    pub delete_btn: Button,
    pub paste_btn: Button,
    text_view: TextView,
    meta_label: Label,
    tags_box: FlowBox,
}

impl PreviewPane {
    pub fn new() -> Self {
        let container = GBox::new(Orientation::Vertical, 8);
        container.set_margin_top(12);
        container.set_margin_bottom(12);
        container.set_margin_start(12);
        container.set_margin_end(12);
        container.set_hexpand(true);

        let meta_label = Label::new(Some("Select a clip to preview"));
        meta_label.set_xalign(0.0);
        meta_label.add_css_class("dim-label");
        meta_label.add_css_class("caption");

        let text_buffer = TextBuffer::new(None);
        let text_view = TextView::with_buffer(&text_buffer);
        text_view.set_editable(false);
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk4::WrapMode::WordChar);
        text_view.add_css_class("card");

        let scroll = ScrolledWindow::builder()
            .child(&text_view)
            .vexpand(true)
            .build();

        let tags_box = FlowBox::new();
        tags_box.set_selection_mode(gtk4::SelectionMode::None);
        tags_box.set_row_spacing(4);
        tags_box.set_column_spacing(4);

        let actions = GBox::new(Orientation::Horizontal, 6);
        actions.set_halign(gtk4::Align::End);

        let pin_btn = Button::with_label("📌 Pin");
        pin_btn.add_css_class("flat");

        let delete_btn = Button::with_label("🗑 Delete");
        delete_btn.add_css_class("flat");
        delete_btn.add_css_class("destructive-action");

        let paste_btn = Button::with_label("⏎ Paste");
        paste_btn.add_css_class("suggested-action");

        actions.append(&pin_btn);
        actions.append(&delete_btn);
        actions.append(&paste_btn);

        container.append(&meta_label);
        container.append(&scroll);
        container.append(&tags_box);
        container.append(&actions);

        Self {
            container,
            pin_btn,
            delete_btn,
            paste_btn,
            text_view,
            meta_label,
            tags_box,
        }
    }

    pub fn show_clip(&self, detail: &ClipDetail) {
        self.meta_label.set_text(&format!(
            "{} · {} · {}",
            detail.content_type,
            if detail.source_app.is_empty() {
                "unknown"
            } else {
                &detail.source_app
            },
            friendly_age(detail.created_at)
        ));

        let text = match detail.content_type.as_str() {
            "text/plain" | "text/html" | "text/markdown" => {
                String::from_utf8_lossy(&detail.content).to_string()
            }
            t if t.starts_with("image/") => "[Image — cannot display inline]".to_string(),
            "application/file" => {
                format!("[File: {}]", String::from_utf8_lossy(&detail.content))
            }
            _ => format!("[{} — {} bytes]", detail.content_type, detail.content.len()),
        };

        self.text_view.buffer().set_text(&text);

        if detail.pinned {
            self.pin_btn.set_label("📌 Unpin");
        } else {
            self.pin_btn.set_label("📌 Pin");
        }

        while let Some(child) = self.tags_box.first_child() {
            self.tags_box.remove(&child);
        }
        for tag in &detail.tags {
            let pill = Label::new(Some(tag));
            pill.add_css_class("tag-pill");
            self.tags_box.append(&pill);
        }
    }

    pub fn clear(&self) {
        self.text_view.buffer().set_text("");
        self.meta_label.set_text("Select a clip to preview");
    }
}

fn friendly_age(created_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let secs = now - created_at;
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{} hr ago", secs / 3600)
    } else {
        format!("{} days ago", secs / 86400)
    }
}
