use gtk4::prelude::*;
use gtk4::{ListBox, ListBoxRow, ScrolledWindow, SelectionMode};

use crate::app::clip_row::ClipRow;
use crate::dbus::ClipSummary;
use crate::util::sort_clips;

// GObject qdata key — used to attach the clip id to each ListBoxRow so
// selection/keyboard handlers can recover the id without maintaining a
// parallel index.
const CLIP_ID_KEY: &str = "clip-id";

pub struct ClipList {
    pub scroll: ScrolledWindow,
    list_box: ListBox,
}

impl ClipList {
    pub fn new() -> Self {
        let list_box = ListBox::new();
        list_box.set_selection_mode(SelectionMode::Single);
        // Single-click previews (row-selected); double-click / Enter
        // activates (row-activated) → paste + close.
        list_box.set_activate_on_single_click(false);
        list_box.add_css_class("navigation-sidebar");

        let scroll = ScrolledWindow::builder()
            .child(&list_box)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .width_request(320)
            .build();

        Self { scroll, list_box }
    }

    /// Replace all rows. Pinned clips sort first; pinned rows get the
    /// `pinned-row` CSS class plus an inline pin badge. No section-header
    /// rows — they would break index-based lookup and add complexity.
    pub fn populate<F>(&self, clips: &[ClipSummary], on_delete: F)
    where
        F: Fn(i64) + Clone + 'static,
    {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        for clip in sort_clips(clips) {
            self.append_clip(clip, on_delete.clone());
        }
    }

    fn append_clip<F>(&self, clip: &ClipSummary, on_delete: F)
    where
        F: Fn(i64) + 'static,
    {
        let clip_row = ClipRow::new(clip);
        let id = clip.id;
        clip_row.delete_btn.connect_clicked(move |_| on_delete(id));

        let row = ListBoxRow::new();
        row.set_child(Some(&clip_row.container));
        // Safety: qdata key is unique to this module; i64 is Copy and
        // outlives any row-hosted closure because it's copied in.
        unsafe {
            row.set_data::<i64>(CLIP_ID_KEY, id);
        }
        self.list_box.append(&row);
    }

    pub fn connect_row_selected<F>(&self, f: F)
    where
        F: Fn(i64) + 'static,
    {
        self.list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                if let Some(id) = row_clip_id(row) {
                    f(id);
                }
            }
        });
    }

    pub fn selected_clip_id(&self) -> Option<i64> {
        self.list_box.selected_row().and_then(|r| row_clip_id(&r))
    }

    pub fn connect_row_activated<F>(&self, f: F)
    where
        F: Fn(i64) + 'static,
    {
        self.list_box.connect_row_activated(move |_, row| {
            if let Some(id) = row_clip_id(row) {
                f(id);
            }
        });
    }
}

fn row_clip_id(row: &ListBoxRow) -> Option<i64> {
    // Safety: only rows created by `append_clip` carry this qdata, and we
    // always store an i64 under CLIP_ID_KEY.
    let ptr = unsafe { row.data::<i64>(CLIP_ID_KEY)? };
    Some(unsafe { *ptr.as_ref() })
}
