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
    pub zone_widgets: RefCell<Vec<(u32, gtk4::Widget)>>,
}

impl EditorView {
    pub fn new(
        app: &gtk4::Application,
        proxy: ZonesProxy<'static>,
        monitor_key: String,
        layout: LayoutWire,
        all_layouts: Vec<LayoutSummaryWire>,
    ) -> Rc<Self> {
        let Some(display) = gdk::Display::default() else {
            panic!("editor: no default display");
        };
        let monitor = monitor_for_key(&display, &monitor_key);
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
        });

        view.build_toolbar(&all_layouts);
        view.wire_canvas_drag();
        view.rerender();
        view
    }

    /// Rebuild all zone rectangles + divider handles from scratch.
    pub fn rerender(self: &Rc<Self>) {
        for (_, w) in self.zone_widgets.borrow().iter() {
            self.canvas.remove(w);
        }
        self.zone_widgets.borrow_mut().clear();

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

        // Apply/Cancel/Reset/New/Save-as/dropdown/gap wiring is Task 15 (Group E).
        let _ = (new_btn, saveas, reset, gap_spin, apply_btn, cancel, dropdown);
    }

    fn wire_canvas_drag(self: &Rc<Self>) {
        // Populated in Task 14
    }

    fn build_divider_handles(self: &Rc<Self>) {
        use gtk4::GestureDrag;

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
            self.zone_widgets.borrow_mut().push((0, handle.clone().upcast()));

            let drag = GestureDrag::new();
            let view = Rc::downgrade(self);
            let axis_copy = axis;
            drag.connect_drag_update(move |_g, dx, dy| {
                if let Some(v) = view.upgrade() {
                    let delta = match axis_copy {
                        crate::editor::state::Axis::Vertical   => dx / v.monitor_w as f64,
                        crate::editor::state::Axis::Horizontal => dy / v.monitor_h as f64,
                    };
                    v.state.borrow_mut().move_divider(first_idx, second_idx, axis_copy, delta);
                    v.rerender();
                }
            });
            handle.add_controller(drag);
        }
    }
}
