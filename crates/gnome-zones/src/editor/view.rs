use crate::dbus::{LayoutSummaryWire, LayoutWire, ZonesProxy};
use crate::editor::state::{EditorState, Zone};
use crate::overlay::{build_overlay, monitor_for_key, KeyMode};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{Align, ApplicationWindow, Box as GBox, Fixed, GestureClick, Label, Orientation};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct EditorView {
    pub window: ApplicationWindow,
    pub canvas: Fixed,
    pub toolbar_container: GBox,
    pub state: Rc<RefCell<EditorState>>,
    pub monitor_key: String,
    pub monitor_w: i32,
    pub monitor_h: i32,
    pub proxy: ZonesProxy<'static>,
    pub zone_widgets: RefCell<Vec<(u32, gtk4::Widget)>>,    // zone rectangles only
    pub divider_widgets: RefCell<Vec<gtk4::Widget>>,
    pub ghost_widget: RefCell<Option<gtk4::Widget>>,
}

impl EditorView {
    pub fn new(
        app: &gtk4::Application,
        proxy: ZonesProxy<'static>,
        monitor_key: String,
        layout: LayoutWire,
        all_layouts: Vec<LayoutSummaryWire>,
    ) -> Option<Rc<Self>> {
        let display = gdk::Display::default()?;
        let monitor = monitor_for_key(&display, &monitor_key)?;
        let geo = monitor.geometry();
        let monitor_w = geo.width();
        let monitor_h = geo.height();

        let window = build_overlay(app, &monitor, KeyMode::Exclusive);
        window.set_title(Some("gnome-zones editor"));

        let root = GBox::new(Orientation::Vertical, 0);
        root.add_css_class("gnome-zones-editor-backdrop");
        root.set_hexpand(true);
        root.set_vexpand(true);

        let canvas = Fixed::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);

        let toolbar_container = GBox::new(Orientation::Horizontal, 8);
        toolbar_container.set_halign(Align::Center);
        toolbar_container.set_valign(Align::End);
        toolbar_container.set_margin_bottom(32);

        root.append(&canvas);
        root.append(&toolbar_container);
        window.set_child(Some(&root));

        let state = Rc::new(RefCell::new(EditorState::from_layout(&layout)));
        let view = Rc::new(Self {
            window,
            canvas,
            toolbar_container,
            state,
            monitor_key,
            monitor_w,
            monitor_h,
            proxy,
            zone_widgets: RefCell::new(Vec::new()),
            divider_widgets: RefCell::new(Vec::new()),
            ghost_widget: RefCell::new(None),
        });

        view.build_toolbar(&all_layouts);
        view.wire_canvas_drag();
        view.rerender();
        Some(view)
    }

    /// Rebuild all zone rectangles + divider handles from scratch.
    pub fn rerender(self: &Rc<Self>) {
        // Tear down zones
        for (_, w) in self.zone_widgets.borrow().iter() {
            self.canvas.remove(w);
        }
        self.zone_widgets.borrow_mut().clear();
        // Tear down dividers
        for w in self.divider_widgets.borrow().iter() {
            self.canvas.remove(w);
        }
        self.divider_widgets.borrow_mut().clear();
        // (ghost_widget is NOT touched — it survives rerender intentionally)

        let state = self.state.borrow();
        for zone in &state.zones {
            let widget = self.build_zone_widget(zone, state.selected == Some(zone.zone_index));
            let zx = (zone.x * self.monitor_w as f64) as i32;
            let zy = (zone.y * self.monitor_h as f64) as i32;
            self.canvas.put(&widget, zx as f64, zy as f64);
            self.zone_widgets.borrow_mut().push((zone.zone_index, widget));
        }
        drop(state);

        self.build_divider_handles();
    }

    fn build_zone_widget(self: &Rc<Self>, zone: &Zone, selected: bool) -> gtk4::Widget {
        let b = GBox::new(Orientation::Vertical, 0);
        let zw = (zone.w * self.monitor_w as f64) as i32;
        let zh = (zone.h * self.monitor_h as f64) as i32;
        b.set_size_request(zw, zh);
        b.add_css_class("gnome-zones-zone");
        if selected {
            b.add_css_class("gnome-zones-zone-selected");
        }

        let num = Label::new(Some(&zone.zone_index.to_string()));
        num.add_css_class("gnome-zones-zone-number");
        num.set_halign(Align::Center);
        num.set_valign(Align::Center);
        num.set_hexpand(true);
        num.set_vexpand(true);
        b.append(&num);

        let zone_index = zone.zone_index;
        let view = Rc::downgrade(self);
        let click = GestureClick::new();
        click.set_button(1);
        click.connect_pressed(move |_g, _n, _x, _y| {
            if let Some(v) = view.upgrade() {
                v.state.borrow_mut().select(zone_index);
                v.rerender();
            }
        });
        b.add_controller(click);

        b.upcast()
    }

    pub(crate) fn build_toolbar(self: &Rc<Self>, all_layouts: &[LayoutSummaryWire]) {
        use gtk4::{Button, DropDown, SpinButton, StringList};

        let names: Vec<&str> = all_layouts.iter().map(|l| l.name.as_str()).collect();
        let model = StringList::new(&names);
        let dropdown = DropDown::new(Some(model), gtk4::Expression::NONE);
        let current_id = self.state.borrow().layout_id;
        if let Some(id) = current_id {
            if let Some(pos) = all_layouts.iter().position(|l| l.id == id) {
                dropdown.set_selected(pos as u32);
            }
        }
        self.toolbar_container.append(&dropdown);

        let new_btn   = Button::with_label("+ New from current");
        let saveas    = Button::with_label("Save as\u{2026}");
        let reset     = Button::with_label("Reset");
        let split_h   = Button::with_label("+ Split horizontal");
        let split_v   = Button::with_label("+ Split vertical");
        let del       = Button::with_label("Delete");
        let gap_spin  = SpinButton::with_range(0.0, 64.0, 1.0);
        gap_spin.set_value(8.0);
        let apply_btn = Button::with_label("Apply");
        apply_btn.add_css_class("suggested-action");
        let cancel    = Button::with_label("Cancel");

        self.toolbar_container.append(&new_btn);
        self.toolbar_container.append(&saveas);
        self.toolbar_container.append(&reset);
        self.toolbar_container.append(&split_h);
        self.toolbar_container.append(&split_v);
        self.toolbar_container.append(&del);
        self.toolbar_container.append(&gap_spin);
        self.toolbar_container.append(&apply_btn);
        self.toolbar_container.append(&cancel);

        {
            let view = Rc::downgrade(self);
            split_h.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() {
                    v.state.borrow_mut().split_horizontal();
                    v.rerender();
                }
            });
        }
        {
            let view = Rc::downgrade(self);
            split_v.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() {
                    v.state.borrow_mut().split_vertical();
                    v.rerender();
                }
            });
        }
        {
            let view = Rc::downgrade(self);
            del.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() {
                    v.state.borrow_mut().delete_selected();
                    v.rerender();
                }
            });
        }

        // Gap spinner → daemon setting
        {
            let proxy = self.proxy.clone();
            gap_spin.connect_value_changed(move |sb| {
                let value = sb.value_as_int().to_string();
                let proxy = proxy.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    if let Err(e) = proxy.set_setting("gap_px", &value).await {
                        tracing::warn!(error = %e, "editor: set_setting(gap_px) failed");
                    }
                });
            });
        }

        // Reset
        {
            let view = Rc::downgrade(self);
            reset.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() {
                    v.state.borrow_mut().reset();
                    v.rerender();
                }
            });
        }

        // + New from current — clears layout_id, renames
        {
            let view = Rc::downgrade(self);
            new_btn.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() {
                    let mut st = v.state.borrow_mut();
                    st.layout_id = None;
                    st.is_preset = false;
                    st.name = format!("{} (copy)", st.name);
                    drop(st);
                    v.rerender();
                }
            });
        }

        // Save as… (prompt name, forks current zones into new layout)
        {
            let view = Rc::downgrade(self);
            saveas.connect_clicked(move |_| {
                let Some(v) = view.upgrade() else { return; };
                v.show_save_as_dialog();
            });
        }

        // Cancel
        {
            let view = Rc::downgrade(self);
            cancel.connect_clicked(move |_| {
                if let Some(v) = view.upgrade() { v.window.close(); }
            });
        }

        // Apply
        {
            let view = Rc::downgrade(self);
            apply_btn.connect_clicked(move |_| {
                let Some(v) = view.upgrade() else { return; };
                v.apply_and_close();
            });
        }

        // Layout dropdown → switch layout (fetches via D-Bus)
        {
            let proxy = self.proxy.clone();
            let view = Rc::downgrade(self);
            let layouts = all_layouts.to_vec();
            dropdown.connect_selected_notify(move |dd| {
                let Some(v) = view.upgrade() else { return; };
                let Some(id) = layouts.get(dd.selected() as usize).map(|l| l.id) else { return; };
                let proxy = proxy.clone();
                let view_ = Rc::downgrade(&v);
                gtk4::glib::MainContext::default().spawn_local(async move {
                    match proxy.get_layout(id).await {
                        Ok(layout) => {
                            if let Some(v) = view_.upgrade() {
                                *v.state.borrow_mut() = EditorState::from_layout(&layout);
                                v.rerender();
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "editor: get_layout failed on dropdown change");
                        }
                    }
                });
            });
        }
    }

    /// Tear down and rebuild the toolbar (e.g. after Save-as adds a new layout).
    fn rebuild_toolbar(self: &Rc<Self>, all_layouts: &[LayoutSummaryWire]) {
        while let Some(child) = self.toolbar_container.first_child() {
            self.toolbar_container.remove(&child);
        }
        self.build_toolbar(all_layouts);
    }

    fn wire_canvas_drag(self: &Rc<Self>) {
        use gtk4::GestureDrag;

        let drag = GestureDrag::new();
        {
            let view_w = Rc::downgrade(self);
            drag.connect_drag_begin(move |_g, start_x, start_y| {
                let Some(view) = view_w.upgrade() else { return; };
                let fx = start_x / view.monitor_w as f64;
                let fy = start_y / view.monitor_h as f64;
                let state = view.state.borrow();
                let inside_existing = state.zones.iter().any(|z|
                    fx >= z.x && fx <= z.x + z.w && fy >= z.y && fy <= z.y + z.h
                );
                drop(state);
                if inside_existing { return; }

                let g = GBox::new(Orientation::Vertical, 0);
                g.add_css_class("gnome-zones-zone-ghost");
                g.set_size_request(1, 1);
                view.canvas.put(&g, start_x, start_y);
                *view.ghost_widget.borrow_mut() = Some(g.upcast());
            });
        }

        {
            let view_w = Rc::downgrade(self);
            drag.connect_drag_update(move |g, dx, dy| {
                let Some(view) = view_w.upgrade() else { return; };
                let Some(w) = view.ghost_widget.borrow().clone() else { return; };
                let (sx, sy) = g.start_point().unwrap_or((0.0, 0.0));
                let x0 = sx.min(sx + dx);
                let y0 = sy.min(sy + dy);
                let rw = dx.abs().max(1.0) as i32;
                let rh = dy.abs().max(1.0) as i32;
                w.set_size_request(rw, rh);
                view.canvas.move_(&w, x0, y0);
            });
        }

        {
            let view_w = Rc::downgrade(self);
            drag.connect_drag_end(move |g, dx, dy| {
                let Some(view) = view_w.upgrade() else { return; };
                if let Some(w) = view.ghost_widget.borrow_mut().take() {
                    view.canvas.remove(&w);
                }
                let (sx, sy) = g.start_point().unwrap_or((0.0, 0.0));
                let fx0 = sx.min(sx + dx) / view.monitor_w as f64;
                let fy0 = sy.min(sy + dy) / view.monitor_h as f64;
                let fw  = dx.abs() / view.monitor_w as f64;
                let fh  = dy.abs() / view.monitor_h as f64;
                if fw > 0.05 && fh > 0.05 {
                    view.state.borrow_mut().add_zone(fx0, fy0, fw, fh);
                    view.rerender();
                }
            });
        }

        self.canvas.add_controller(drag);
    }

    fn build_divider_handles(self: &Rc<Self>) {
        use gtk4::GestureDrag;
        use std::cell::Cell;

        const HANDLE_THICKNESS: i32 = 6;
        let edges = self.state.borrow().shared_edges();
        for (first_idx, second_idx, axis) in edges {
            let state = self.state.borrow();
            let Some(a) = state.zones.iter().find(|z| z.zone_index == first_idx).copied() else { continue; };
            let Some(b) = state.zones.iter().find(|z| z.zone_index == second_idx).copied() else { continue; };
            drop(state);

            let handle = GBox::new(Orientation::Vertical, 0);
            handle.add_css_class("gnome-zones-divider");
            let (px, py, sz_w, sz_h) = match axis {
                crate::editor::state::Axis::Vertical => {
                    let x = (b.x * self.monitor_w as f64) as i32 - HANDLE_THICKNESS / 2;
                    let y = (a.y * self.monitor_h as f64) as i32;
                    let h = (a.h * self.monitor_h as f64) as i32;
                    (x, y, HANDLE_THICKNESS, h)
                }
                crate::editor::state::Axis::Horizontal => {
                    let x = (a.x * self.monitor_w as f64) as i32;
                    let y = (b.y * self.monitor_h as f64) as i32 - HANDLE_THICKNESS / 2;
                    let w = (a.w * self.monitor_w as f64) as i32;
                    (x, y, w, HANDLE_THICKNESS)
                }
            };
            handle.set_size_request(sz_w, sz_h);
            self.canvas.put(&handle, px as f64, py as f64);
            self.divider_widgets.borrow_mut().push(handle.clone().upcast());

            // Cumulative deltas are what GTK delivers; we apply incremental.
            let last = Rc::new(Cell::new((0.0_f64, 0.0_f64)));

            let drag = GestureDrag::new();
            {
                let last = last.clone();
                drag.connect_drag_begin(move |_g, _sx, _sy| {
                    last.set((0.0, 0.0));
                });
            }
            {
                let view = Rc::downgrade(self);
                let last = last.clone();
                drag.connect_drag_update(move |_g, dx, dy| {
                    let Some(v) = view.upgrade() else { return; };
                    let (last_dx, last_dy) = last.get();
                    let incr_dx = dx - last_dx;
                    let incr_dy = dy - last_dy;
                    last.set((dx, dy));

                    let delta = match axis {
                        crate::editor::state::Axis::Vertical   => incr_dx / v.monitor_w as f64,
                        crate::editor::state::Axis::Horizontal => incr_dy / v.monitor_h as f64,
                    };
                    v.state.borrow_mut().move_divider(first_idx, second_idx, axis, delta);
                    v.refresh_divider_drag(first_idx, second_idx, axis);
                });
            }
            {
                let view = Rc::downgrade(self);
                drag.connect_drag_end(move |_g, _dx, _dy| {
                    if let Some(v) = view.upgrade() {
                        // Full rebuild so divider handle list is accurate for subsequent drags
                        v.rerender();
                    }
                });
            }
            handle.add_controller(drag);
        }
    }

    /// In-place visual update for a divider drag: resize the two affected zone widgets
    /// and move the corresponding handle. Avoids full rerender during drag (which would
    /// destroy the gesture target).
    fn refresh_divider_drag(self: &Rc<Self>, first_idx: u32, second_idx: u32, _axis: crate::editor::state::Axis) {
        let state = self.state.borrow();
        let Some(a) = state.zones.iter().find(|z| z.zone_index == first_idx).copied() else { return; };
        let Some(b) = state.zones.iter().find(|z| z.zone_index == second_idx).copied() else { return; };
        drop(state);

        // Update zone widgets
        for (idx, widget) in self.zone_widgets.borrow().iter() {
            if *idx == first_idx {
                let zw = (a.w * self.monitor_w as f64) as i32;
                let zh = (a.h * self.monitor_h as f64) as i32;
                widget.set_size_request(zw, zh);
                let zx = (a.x * self.monitor_w as f64) as i32;
                let zy = (a.y * self.monitor_h as f64) as i32;
                self.canvas.move_(widget, zx as f64, zy as f64);
            } else if *idx == second_idx {
                let zw = (b.w * self.monitor_w as f64) as i32;
                let zh = (b.h * self.monitor_h as f64) as i32;
                widget.set_size_request(zw, zh);
                let zx = (b.x * self.monitor_w as f64) as i32;
                let zy = (b.y * self.monitor_h as f64) as i32;
                self.canvas.move_(widget, zx as f64, zy as f64);
            }
        }

        // We do NOT move the handle here — the drag gesture is attached to it
        // and moving the widget during an in-flight drag can confuse the gesture.
        // The handle renders in the right visual spot relative to the pointer;
        // on drag_end, rerender() rebuilds divider handles at canonical positions.
    }

    fn apply_and_close(self: &Rc<Self>) {
        let state = self.state.borrow().clone();
        let proxy = self.proxy.clone();
        let monitor_key = self.monitor_key.clone();
        let window = self.window.clone();

        gtk4::glib::MainContext::default().spawn_local(async move {
            let zones: Vec<_> = state.zones.iter().map(Into::into).collect();
            let id = if let Some(id) = state.layout_id {
                if state.is_preset {
                    // Preset is read-only: fork it instead of updating.
                    match proxy.create_layout(&state.name, zones).await {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!(error = %e, "apply: create failed (preset fork)");
                            return; // window stays open
                        }
                    }
                } else {
                    if let Err(e) = proxy.update_layout(id, &state.name, zones).await {
                        tracing::error!(error = %e, "apply: update failed");
                        return; // window stays open
                    }
                    id
                }
            } else {
                match proxy.create_layout(&state.name, zones).await {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::error!(error = %e, "apply: create failed");
                        return; // window stays open
                    }
                }
            };
            if let Err(e) = proxy.assign_layout(&monitor_key, id).await {
                tracing::error!(error = %e, "apply: assign failed — layout saved but not activated");
                return; // window stays open; user can inspect and retry
            }
            window.close();
        });
    }

    fn show_save_as_dialog(self: &Rc<Self>) {
        use libadwaita::prelude::*;
        use libadwaita::{AlertDialog, ResponseAppearance};

        let dialog = AlertDialog::new(
            Some("Save layout as"),
            Some("Enter a name for the new layout."),
        );

        let entry = gtk4::Entry::builder()
            .text(&format!("{} (copy)", self.state.borrow().name))
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let proxy = self.proxy.clone();
        let view = Rc::downgrade(self);
        let entry_clone = entry.clone();
        dialog.connect_response(None, move |dialog, response| {
            if response == "save" {
                let name = entry_clone.text().to_string();
                let Some(v) = view.upgrade() else { dialog.close(); return; };
                let zones: Vec<_> = v.state.borrow().zones.iter().map(Into::into).collect();
                let proxy = proxy.clone();
                let view_ = view.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    match proxy.create_layout(&name, zones).await {
                        Ok(id) => {
                            let Some(v) = view_.upgrade() else { return; };
                            match proxy.get_layout(id).await {
                                Ok(layout) => {
                                    *v.state.borrow_mut() = EditorState::from_layout(&layout);
                                    v.rerender();
                                }
                                Err(e) => tracing::warn!(error = %e, "save_as: get_layout failed"),
                            }
                            // Refresh dropdown so the new layout appears.
                            match proxy.list_layouts().await {
                                Ok(layouts) => v.rebuild_toolbar(&layouts),
                                Err(e) => tracing::warn!(error = %e, "save_as: list_layouts refresh failed"),
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "save_as: create_layout failed"),
                    }
                });
            }
            dialog.close();
        });

        dialog.present(Some(&self.window));
    }
}

/// Public entry point: fetch the current state for `monitor_key` and present the editor.
pub fn show(
    app: &gtk4::Application,
    proxy: ZonesProxy<'static>,
    monitor_key: String,
) {
    let app_weak = app.downgrade();
    let proxy_fetch = proxy.clone();
    let mk = monitor_key.clone();

    gtk4::glib::MainContext::default().spawn_local(async move {
        let layouts = match proxy_fetch.list_layouts().await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(error = %e, "editor: list_layouts failed");
                return;
            }
        };
        let active = match proxy_fetch.get_active_layout(&mk).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(error = %e, "editor: get_active_layout failed");
                return;
            }
        };
        let Some(app) = app_weak.upgrade() else { return; };
        let Some(view) = EditorView::new(&app, proxy, mk, active, layouts) else {
            tracing::warn!("editor: failed to build view (no display or monitors)");
            return;
        };
        view.window.present();
    });
}
