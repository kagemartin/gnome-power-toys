use gtk4::prelude::*;
use gtk4::{Box as GBox, Button, Label, Orientation};

use crate::dbus::ClipSummary;
use crate::util::{friendly_age, friendly_type, type_icon};

pub struct ClipRow {
    pub container: GBox,
    pub delete_btn: Button,
    pub id: i64,
}

impl ClipRow {
    pub fn new(clip: &ClipSummary) -> Self {
        let container = GBox::new(Orientation::Horizontal, 6);
        container.set_margin_top(4);
        container.set_margin_bottom(4);
        container.set_margin_start(8);
        container.set_margin_end(8);

        let icon_label = Label::new(Some(type_icon(&clip.content_type)));

        let text_box = GBox::new(Orientation::Vertical, 2);
        text_box.set_hexpand(true);

        let preview = Label::new(Some(&clip.preview));
        preview.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        preview.set_xalign(0.0);
        preview.set_max_width_chars(40);
        preview.add_css_class("clip-preview");

        let meta = Label::new(Some(&format!(
            "{} · {} · {}",
            friendly_type(&clip.content_type),
            if clip.source_app.is_empty() {
                "unknown"
            } else {
                &clip.source_app
            },
            friendly_age(clip.created_at)
        )));
        meta.set_xalign(0.0);
        meta.add_css_class("dim-label");
        meta.add_css_class("caption");

        text_box.append(&preview);
        text_box.append(&meta);

        let delete_btn = Button::with_label("✕");
        delete_btn.add_css_class("flat");
        delete_btn.add_css_class("destructive-action");

        if clip.pinned {
            container.add_css_class("pinned-row");
            // Inline pin badge so users can spot pinned items without a section header.
            let pin_badge = Label::new(Some("📌"));
            pin_badge.add_css_class("caption");
            container.append(&pin_badge);
        }

        container.append(&icon_label);
        container.append(&text_box);
        container.append(&delete_btn);

        Self {
            container,
            delete_btn,
            id: clip.id,
        }
    }
}

