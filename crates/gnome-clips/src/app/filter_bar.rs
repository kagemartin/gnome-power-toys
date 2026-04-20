use gtk4::prelude::*;
use gtk4::{Box as GBox, Entry, Orientation, ToggleButton};

#[derive(Clone)]
pub struct FilterBar {
    pub container: GBox,
    pub search: Entry,
    pub filter_all: ToggleButton,
    pub filter_text: ToggleButton,
    pub filter_image: ToggleButton,
    pub filter_file: ToggleButton,
    pub filter_html: ToggleButton,
    pub filter_markdown: ToggleButton,
    pub filter_pinned: ToggleButton,
}

impl FilterBar {
    pub fn new() -> Self {
        let container = GBox::new(Orientation::Horizontal, 6);
        container.set_margin_top(8);
        container.set_margin_bottom(8);
        container.set_margin_start(12);
        container.set_margin_end(12);

        let search = Entry::builder()
            .placeholder_text("Search clipboard history…")
            .hexpand(true)
            .build();

        let filter_all = pill_toggle("All");
        let filter_text = pill_toggle("Text");
        let filter_image = pill_toggle("Image");
        let filter_file = pill_toggle("File");
        let filter_html = pill_toggle("HTML");
        let filter_markdown = pill_toggle("MD");
        let filter_pinned = pill_toggle("📌 Pinned");

        filter_all.set_active(true);

        container.append(&search);
        for btn in [
            &filter_all,
            &filter_text,
            &filter_image,
            &filter_file,
            &filter_html,
            &filter_markdown,
            &filter_pinned,
        ] {
            container.append(btn);
        }

        Self {
            container,
            search,
            filter_all,
            filter_text,
            filter_image,
            filter_file,
            filter_html,
            filter_markdown,
            filter_pinned,
        }
    }

    /// Returns the active D-Bus filter string (empty = all).
    pub fn active_filter(&self) -> &'static str {
        if self.filter_pinned.is_active() {
            return "pinned";
        }
        if self.filter_text.is_active() {
            return "text/plain";
        }
        if self.filter_image.is_active() {
            return "image/*";
        }
        if self.filter_file.is_active() {
            return "application/file";
        }
        if self.filter_html.is_active() {
            return "text/html";
        }
        if self.filter_markdown.is_active() {
            return "text/markdown";
        }
        ""
    }
}

fn pill_toggle(label: &str) -> ToggleButton {
    let btn = ToggleButton::with_label(label);
    btn.add_css_class("pill");
    btn
}
