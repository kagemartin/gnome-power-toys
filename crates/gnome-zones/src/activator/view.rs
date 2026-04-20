use crate::activator::state::{handle_key, ActivatorAction};
use crate::dbus::ZonesProxy;
use crate::overlay::{build_overlay, monitor_for_key, KeyMode};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{Align, EventControllerKey, Fixed, Label};

const ACTIVATOR_TIMEOUT_MS: u64 = 3000;

/// Spawn the activator overlay for the given monitor_key.
///
/// `paused` — if true, renders a "Paused" banner and ignores digits.
pub fn show(
    app: &gtk4::Application,
    proxy: ZonesProxy<'static>,
    monitor_key: String,
    paused: bool,
) {
    let Some(display) = gdk::Display::default() else {
        tracing::warn!("activator: no default display; cannot show overlay");
        return;
    };
    let monitor = monitor_for_key(&display, &monitor_key);

    let window = build_overlay(app, &monitor, KeyMode::OnDemand);
    window.set_title(Some("gnome-zones activator"));

    let app_weak = app.downgrade();
    let window_weak = window.downgrade();
    let proxy_fetch = proxy.clone();
    let mk = monitor_key.clone();

    gtk4::glib::MainContext::default().spawn_local(async move {
        let layout = match proxy_fetch.get_active_layout(&mk).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(error = %e, "activator: failed to fetch active layout");
                if let Some(w) = window_weak.upgrade() { w.close(); }
                return;
            }
        };
        if app_weak.upgrade().is_none() { return; }
        let Some(window) = window_weak.upgrade() else { return; };

        let monitor_geo = monitor.geometry();
        let monitor_w = monitor_geo.width();
        let monitor_h = monitor_geo.height();

        let fixed = Fixed::new();
        fixed.add_css_class("gnome-zones-overlay");

        if paused {
            let banner = Label::new(Some("gnome-zones is paused"));
            banner.add_css_class("gnome-zones-zone-number");
            banner.set_halign(Align::Center);
            banner.set_valign(Align::Center);
            fixed.put(&banner, (monitor_w / 2 - 300) as f64, (monitor_h / 2 - 80) as f64);
        } else {
            for zone in &layout.zones {
                let zx = (zone.x * monitor_w as f64) as i32;
                let zy = (zone.y * monitor_h as f64) as i32;
                let zw = (zone.w * monitor_w as f64) as i32;
                let zh = (zone.h * monitor_h as f64) as i32;

                let rect = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                rect.set_size_request(zw, zh);
                rect.add_css_class("gnome-zones-zone");

                let num = Label::new(Some(&zone.zone_index.to_string()));
                num.add_css_class("gnome-zones-zone-number");
                num.set_halign(Align::Center);
                num.set_valign(Align::Center);
                num.set_hexpand(true);
                num.set_vexpand(true);
                rect.append(&num);

                fixed.put(&rect, zx as f64, zy as f64);
            }
        }
        window.set_child(Some(&fixed));

        let zone_count = layout.zones.len() as u32;
        let proxy_keys = proxy.clone();
        let window_keys = window.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_ctrl, keyval, _keycode, state| {
            let name = keyval.name().map(|s| s.to_string()).unwrap_or_default();
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            let action = handle_key(&name, shift, zone_count, paused);
            match action {
                ActivatorAction::Snap { zone_index, span, dismiss } => {
                    let proxy = proxy_keys.clone();
                    gtk4::glib::MainContext::default().spawn_local(async move {
                        if let Err(e) = proxy.snap_focused_to_zone(zone_index, span).await {
                            tracing::warn!(error = %e, "activator: snap failed");
                        }
                    });
                    if dismiss { window_keys.close(); }
                }
                ActivatorAction::Dismiss => window_keys.close(),
                ActivatorAction::Ignore => {}
            }
            gtk4::glib::Propagation::Stop
        });
        window.add_controller(key_ctrl);

        let window_timeout = window.downgrade();
        gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_millis(ACTIVATOR_TIMEOUT_MS),
            move || {
                if let Some(w) = window_timeout.upgrade() {
                    w.close();
                }
            },
        );

        window.present();
    });

    window.present();
}
