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

    pub(crate) fn build_toolbar(self: &Rc<Self>, _all_layouts: &[LayoutSummaryWire]) {
        // Populated in Task 12
    }

    fn wire_canvas_drag(self: &Rc<Self>) {
        // Populated in Task 14
    }

    fn build_divider_handles(self: &Rc<Self>) {
        // Populated in Task 13 step 3
    }
}
