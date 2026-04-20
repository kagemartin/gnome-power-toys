use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GBox, Orientation, Paned, Separator};
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::app::clip_list::ClipList;
use crate::app::filter_bar::FilterBar;
use crate::app::preview_pane::PreviewPane;
use crate::dbus::ClipsProxy;

pub struct ClipsWindow {
    pub window: ApplicationWindow,
    pub clip_list: Rc<ClipList>,
    pub preview: Rc<PreviewPane>,
    pub proxy: ClipsProxy<'static>,
    pub selected_id: Rc<Cell<Option<i64>>>,
    pub refresh: Rc<RefCell<Box<dyn Fn()>>>,
}

impl ClipsWindow {
    pub fn new(app: &adw::Application, proxy: ClipsProxy<'static>) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Clipboard History")
            .default_width(780)
            .default_height(460)
            .decorated(true)
            .build();

        // Close hides, does not destroy — re-showing is instant.
        window.connect_close_request(|w| {
            w.set_visible(false);
            glib::Propagation::Stop
        });

        let filter_bar = FilterBar::new();
        let clip_list = Rc::new(ClipList::new());
        let preview = Rc::new(PreviewPane::new());
        let selected_id: Rc<Cell<Option<i64>>> = Rc::new(Cell::new(None));

        let vbox = GBox::new(Orientation::Vertical, 0);
        vbox.append(&filter_bar.container);
        vbox.append(&Separator::new(Orientation::Horizontal));

        let paned = Paned::new(Orientation::Horizontal);
        paned.set_start_child(Some(&clip_list.scroll));
        paned.set_end_child(Some(&preview.container));
        paned.set_position(320);
        paned.set_vexpand(true);
        vbox.append(&paned);
        window.set_child(Some(&vbox));

        let refresh: Rc<RefCell<Box<dyn Fn()>>> = {
            let proxy = proxy.clone();
            let clip_list = clip_list.clone();
            let filter_bar = filter_bar.clone();
            Rc::new(RefCell::new(Box::new(move || {
                let proxy_fetch = proxy.clone();
                let proxy_del = proxy.clone();
                let clip_list = clip_list.clone();
                let filter = filter_bar.active_filter().to_string();
                let search = filter_bar.search.text().to_string();
                glib::MainContext::default().spawn_local(async move {
                    match proxy_fetch.get_history(&filter, &search, 0, 200).await {
                        Ok(result) => {
                            let proxy_del = proxy_del.clone();
                            clip_list.populate(&result, move |id| {
                                let proxy = proxy_del.clone();
                                glib::MainContext::default().spawn_local(async move {
                                    let _ = proxy.delete_clip(id).await;
                                });
                            });
                        }
                        Err(e) => tracing::warn!(error = %e, "get_history failed"),
                    }
                });
            }) as Box<dyn Fn()>))
        };

        // Filter pills: one-hot, others deactivate on click.
        let pills = [
            filter_bar.filter_all.clone(),
            filter_bar.filter_text.clone(),
            filter_bar.filter_image.clone(),
            filter_bar.filter_file.clone(),
            filter_bar.filter_html.clone(),
            filter_bar.filter_markdown.clone(),
            filter_bar.filter_pinned.clone(),
        ];
        for (i, pill) in pills.iter().enumerate() {
            let others: Vec<_> = pills
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p.clone())
                .collect();
            let refresh = refresh.clone();
            pill.connect_toggled(move |btn| {
                if btn.is_active() {
                    for p in &others {
                        p.set_active(false);
                    }
                    (refresh.borrow())();
                }
            });
        }

        // Search field changes → refresh.
        {
            let refresh = refresh.clone();
            filter_bar.search.connect_changed(move |_| (refresh.borrow())());
        }

        // Row selection → preview.
        {
            let proxy = proxy.clone();
            let preview = preview.clone();
            let selected_id = selected_id.clone();
            clip_list.connect_row_selected(move |id| {
                selected_id.set(Some(id));
                let proxy = proxy.clone();
                let preview = preview.clone();
                glib::MainContext::default().spawn_local(async move {
                    match proxy.get_clip(id).await {
                        Ok(detail) => preview.show_clip(&detail),
                        Err(e) => tracing::warn!(error = %e, "get_clip failed"),
                    }
                });
            });
        }

        // Pin button in preview pane.
        {
            let proxy = proxy.clone();
            let selected_id = selected_id.clone();
            let refresh = refresh.clone();
            preview.pin_btn.connect_clicked(move |btn| {
                let Some(id) = selected_id.get() else {
                    return;
                };
                // Label reads "📌 Unpin" when currently pinned.
                let currently_pinned = btn
                    .label()
                    .map(|l| l.as_str().contains("Unpin"))
                    .unwrap_or(false);
                let proxy = proxy.clone();
                let refresh = refresh.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = proxy.set_pinned(id, !currently_pinned).await;
                    (refresh.borrow())();
                });
            });
        }

        // Delete button in preview pane.
        {
            let proxy = proxy.clone();
            let selected_id = selected_id.clone();
            preview.delete_btn.connect_clicked(move |_| {
                let Some(id) = selected_id.get() else {
                    return;
                };
                let proxy = proxy.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = proxy.delete_clip(id).await;
                });
            });
        }

        // Paste button: polish pass will wire wl-clipboard; for now, hide window.
        {
            let window_paste = window.clone();
            preview.paste_btn.connect_clicked(move |_| {
                window_paste.set_visible(false);
            });
        }

        // Initial load.
        (refresh.borrow())();

        Self {
            window,
            clip_list,
            preview,
            proxy,
            selected_id,
            refresh,
        }
    }

    pub fn show(&self) {
        self.window.present();
    }
    pub fn hide(&self) {
        self.window.set_visible(false);
    }
    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }
}
