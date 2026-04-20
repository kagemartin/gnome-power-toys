use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GBox, EventControllerKey, Orientation, Paned, Separator};
use libadwaita as adw;

use crate::app::clip_list::ClipList;
use crate::app::filter_bar::FilterBar;
use crate::app::preview_pane::PreviewPane;
use crate::dbus::ClipsProxy;

pub struct ClipsWindow {
    pub window: ApplicationWindow,
    // Kept for show()'s `focus_selected_row` + re-fetch on re-present.
    // The tray, keyboard controller, and D-Bus signal subscriptions all
    // hold their own strong refs to the other widgets via closures, so
    // they stay alive whether we store them here or not.
    pub clip_list: Rc<ClipList>,
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
        // Make the ListBox the window's initial focus target — otherwise
        // GTK picks the first focusable child, which in our layout is a
        // ToggleButton or the paste button.
        gtk4::prelude::GtkWindowExt::set_focus(&window, Some(clip_list.focus_target()));

        // Every time the window is mapped (first show, and each re-show
        // after hiding), re-assert focus on the selected list row.
        {
            let clip_list = clip_list.clone();
            window.connect_map(move |_| {
                clip_list.focus_selected_row();
            });
        }

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

        // Paste is delegated to the daemon's `Paste` method. The daemon
        // owns the system-clipboard selection for the whole session, so
        // ownership outlives the popup hide (which was the reason writing
        // from the UI failed for real paste clients).
        let paste: Rc<dyn Fn(i64) + 'static> = {
            let window = window.clone();
            let proxy = proxy.clone();
            Rc::new(move |id: i64| {
                let window = window.clone();
                let proxy = proxy.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Err(e) = proxy.paste(id).await {
                        tracing::warn!(error = %e, clip_id = id, "daemon paste failed");
                        return;
                    }
                    window.set_visible(false);
                    // Ask the shell extension to re-focus the previously-
                    // focused window and synthesize the paste shortcut.
                    request_inject_paste(&proxy).await;
                });
            })
        };

        // Paste button.
        {
            let paste = paste.clone();
            let selected_id = selected_id.clone();
            preview.paste_btn.connect_clicked(move |_| {
                if let Some(id) = selected_id.get() {
                    paste(id);
                }
            });
        }

        // Double-click / Enter on a row → paste.
        {
            let paste = paste.clone();
            clip_list.connect_row_activated(move |id| paste(id));
        }

        // Keyboard shortcuts. Installed AFTER `paste` is built so Enter
        // can fall through to it when focus isn't on the ListBox row.
        let key_controller = EventControllerKey::new();
        {
            let window_ref = window.clone();
            let proxy_ref = proxy.clone();
            let clip_list_ref = clip_list.clone();
            let paste = paste.clone();
            key_controller.connect_key_pressed(move |_, key, _, modifier| {
                use gtk4::gdk::{Key, ModifierType};
                let ctrl = modifier.contains(ModifierType::CONTROL_MASK);
                match (key, ctrl) {
                    (Key::Escape, _) => {
                        window_ref.set_visible(false);
                        glib::Propagation::Stop
                    }
                    (Key::Return | Key::KP_Enter, false) => {
                        if let Some(id) = clip_list_ref.selected_clip_id() {
                            paste(id);
                        }
                        glib::Propagation::Stop
                    }
                    (Key::Delete, _) => {
                        if let Some(id) = clip_list_ref.selected_clip_id() {
                            let proxy = proxy_ref.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let _ = proxy.delete_clip(id).await;
                            });
                        }
                        glib::Propagation::Stop
                    }
                    (Key::p, true) => {
                        if let Some(id) = clip_list_ref.selected_clip_id() {
                            let proxy = proxy_ref.clone();
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(detail) = proxy.get_clip(id).await {
                                    let _ = proxy.set_pinned(id, !detail.pinned).await;
                                }
                            });
                        }
                        glib::Propagation::Stop
                    }
                    (Key::i, true) => {
                        let proxy = proxy_ref.clone();
                        glib::MainContext::default().spawn_local(async move {
                            let current = proxy.is_incognito().await.unwrap_or(false);
                            let _ = proxy.set_incognito(!current).await;
                        });
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
        }
        window.add_controller(key_controller);

        // Live updates — ClipAdded / ClipDeleted / ClipUpdated refresh the list.
        // ClipUpdated fires after a paste so the MRU reordering is reflected.
        {
            let proxy_sig = proxy.clone();
            let refresh = refresh.clone();
            glib::MainContext::default().spawn_local(async move {
                use futures_util::StreamExt;
                if let Ok(mut stream) = proxy_sig.receive_clip_added().await {
                    while stream.next().await.is_some() {
                        (refresh.borrow())();
                    }
                }
            });
        }
        {
            let proxy_sig = proxy.clone();
            let refresh = refresh.clone();
            glib::MainContext::default().spawn_local(async move {
                use futures_util::StreamExt;
                if let Ok(mut stream) = proxy_sig.receive_clip_deleted().await {
                    while stream.next().await.is_some() {
                        (refresh.borrow())();
                    }
                }
            });
        }
        {
            let proxy_sig = proxy.clone();
            let refresh = refresh.clone();
            glib::MainContext::default().spawn_local(async move {
                use futures_util::StreamExt;
                if let Ok(mut stream) = proxy_sig.receive_clip_updated().await {
                    while stream.next().await.is_some() {
                        (refresh.borrow())();
                    }
                }
            });
        }

        // Initial load.
        (refresh.borrow())();

        Self {
            window,
            clip_list,
            refresh,
        }
    }

    pub fn show(&self) {
        // Re-assert the focus target before present() — the window may
        // have moved focus to another widget (e.g. search Entry after
        // the user typed in it) on the previous activation.
        gtk4::prelude::GtkWindowExt::set_focus(
            &self.window,
            Some(self.clip_list.focus_target()),
        );
        self.window.present();
        // Re-fetch so the MRU-on-paste reordering from the previous
        // dismissal is reflected even if the signal raced.
        (self.refresh.borrow())();
        // Defer a grab to the next main-loop iteration — by then the
        // window is mapped and populate has completed, so focus actually
        // sticks on the first row.
        let clip_list = self.clip_list.clone();
        glib::idle_add_local_once(move || {
            clip_list.focus_selected_row();
        });
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

/// Call the gnome-clips-toggle shell extension's InjectPaste method,
/// which refocuses the pre-popup window and synthesizes Ctrl+V (or
/// Ctrl+Shift+V for terminals) so the chosen clip actually lands in
/// the target app. If the extension isn't installed/running the call
/// fails quietly — the user can still manually Ctrl+V the clipboard.
///
/// Reuses the existing daemon proxy's zbus Connection so we don't
/// spin up a second connection from inside a glib::spawn_local —
/// constructing one there hits zbus's runtime detection and panics.
async fn request_inject_paste(proxy: &ClipsProxy<'_>) {
    let conn = proxy.inner().connection();
    let call = conn
        .call_method(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell/Extensions/GnomeClipsToggle",
            Some("org.gnome.Shell.Extensions.GnomeClipsToggle"),
            "InjectPaste",
            &(),
        )
        .await;
    if let Err(e) = call {
        tracing::warn!(error = %e, "InjectPaste call failed");
    }
}
