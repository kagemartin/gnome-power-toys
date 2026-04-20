use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::ApplicationWindow;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

/// How the overlay should interact with keyboard input.
#[derive(Debug, Clone, Copy)]
pub enum KeyMode {
    /// Grab focus (editor).
    Exclusive,
    /// Receive key events but don't steal focus from the underlying app
    /// (activator — keeps the target window focused).
    OnDemand,
}

/// Build a transparent, borderless window covering the given monitor.
pub fn build_overlay(
    app: &gtk4::Application,
    monitor: &gdk::Monitor,
    key_mode: KeyMode,
) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();

    window.add_css_class("gnome-zones-overlay");

    if gtk4_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_monitor(monitor);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
            window.set_margin(edge, 0);
        }
        window.set_keyboard_mode(match key_mode {
            KeyMode::Exclusive => gtk4_layer_shell::KeyboardMode::Exclusive,
            KeyMode::OnDemand => gtk4_layer_shell::KeyboardMode::OnDemand,
        });
    } else {
        window.fullscreen_on_monitor(monitor);
    }

    window
}

/// Resolve a `gdk::Monitor` for a `monitor_key` produced by the daemon.
/// Matches by connector name (first token of `monitor_key`).
///
/// Returns `None` when GDK reports no monitors (headless, display closed
/// mid-session, etc.). Callers must handle this gracefully rather than
/// panicking — the process stays alive even without a presentable monitor.
pub fn monitor_for_key(display: &gdk::Display, monitor_key: &str) -> Option<gdk::Monitor> {
    let connector = monitor_key.split(':').next().unwrap_or("");
    let monitors = display.monitors();
    let n = monitors.n_items();
    for i in 0..n {
        if let Some(m) = monitors.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
            if m.connector().map(|s| s.as_str() == connector).unwrap_or(false) {
                return Some(m);
            }
        }
    }
    monitors
        .item(0)
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
}
