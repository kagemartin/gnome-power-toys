# gnome-zones UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `gnome-zones`, the GTK4/libadwaita UI process — a full-screen zone editor overlay, a transient activator overlay, and a panel status icon — all driven by D-Bus signals from the running `gnome-zones-daemon`.

**Architecture:** A single GTK4 binary (`gnome-zones`) that (1) owns the panel `AppIndicator3` icon, (2) subscribes to the `org.gnome.Zones` D-Bus service for `ActivatorRequested` and `EditorRequested` signals, and (3) spawns the appropriate overlay window on signal receipt. Overlays are transparent layer-shell windows (Wayland) or `_NET_WM_WINDOW_TYPE_DOCK` windows (X11) that cover one monitor. All persistent state lives in the daemon; the UI fetches layouts / monitors / settings on demand and issues mutating D-Bus calls back.

The editor's *pure logic* (split/delete/renumber/merge) lives in a standalone `editor::state` module that is fully unit-testable. GTK code in `editor::view` only renders that state and translates pointer events into state mutations. Same separation for the activator.

**Tech Stack:** Rust 2021, `gtk4` + `libadwaita` (gtk4-rs), `gtk4-layer-shell` (Wayland overlays), `zbus` 4 with tokio feature (D-Bus client), `tokio` 1, `libayatana-appindicator` 0.9 (panel icon), `futures-util` for signal streams.

**Spec:** `docs/superpowers/specs/2026-04-14-gnome-zones-design.md`

**Prerequisite:** `gnome-zones-daemon` is already complete and running (registers `org.gnome.Zones` on the session bus; emits `ActivatorRequested` / `EditorRequested` on hotkey).

**This is the UI portion of the gnome-zones v1 spec.** Packaging (.deb, Flatpak, gsettings overrides) is not covered here — the daemon already ships a systemd unit; a follow-up packaging plan will bundle both binaries.

---

## File Structure

```
crates/gnome-zones/
├── Cargo.toml
└── src/
    ├── main.rs                   # GtkApplication entry + CLI + signal dispatcher
    ├── error.rs                  # Error + Result types
    ├── dbus/
    │   ├── mod.rs                # Connect helper, re-exports
    │   └── proxy.rs              # zbus proxy for org.gnome.Zones + wire types
    ├── overlay/
    │   ├── mod.rs                # re-exports
    │   └── layer_shell.rs        # Helper: monitor-covering transparent window
    ├── activator/
    │   ├── mod.rs                # re-exports
    │   ├── state.rs              # Pure logic: key → ActivatorAction
    │   └── view.rs               # GTK widget: render + handle input
    ├── editor/
    │   ├── mod.rs                # re-exports
    │   ├── state.rs              # Pure logic: WIP layout, split/delete/renumber/merge
    │   └── view.rs               # GTK widget: render + drag + toolbar
    └── panel/
        ├── mod.rs                # re-exports
        └── indicator.rs          # AppIndicator3 tray icon + right-click menu
```

Each file has one responsibility. `state.rs` modules are pure Rust (no GTK); `view.rs` modules and `indicator.rs` contain all GTK / AppIndicator code.

---

## Task 1: Add gnome-zones crate to workspace

**Files:**
- Modify: `Cargo.toml` (workspace manifest)
- Create: `crates/gnome-zones/Cargo.toml`
- Create: `crates/gnome-zones/src/main.rs`

- [ ] **Step 1: Add crate to workspace**

Edit `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/gnome-zones-daemon",
    "crates/gnome-zones",
]
resolver = "2"
```

- [ ] **Step 2: Create `crates/gnome-zones/Cargo.toml`**

```toml
[package]
name    = "gnome-zones"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-or-later"

[[bin]]
name = "gnome-zones"
path = "src/main.rs"

[dependencies]
gtk4               = { version = "0.9", features = ["v4_12"] }
libadwaita         = { version = "0.7", features = ["v1_5"] }
gtk4-layer-shell   = "0.4"
zbus               = { version = "4",    features = ["tokio"] }
tokio              = { version = "1",    features = ["full"] }
serde              = { version = "1",    features = ["derive"] }
futures-util       = "0.3"
tracing            = "0.1"
tracing-subscriber = { version = "0.3",  features = ["env-filter"] }
thiserror          = "1"
clap               = { version = "4",    features = ["derive"] }

[dependencies.libayatana-appindicator]
version = "0.9"
features = []
```

- [ ] **Step 3: Create stub `crates/gnome-zones/src/main.rs`**

```rust
fn main() {
    println!("gnome-zones stub");
}
```

- [ ] **Step 4: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success. Install deps if missing:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libgtk-layer-shell-dev libayatana-appindicator3-dev
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/gnome-zones/
git commit -m "chore(zones): scaffold gnome-zones UI crate"
```

---

## Task 2: Error types + D-Bus proxy

**Files:**
- Create: `crates/gnome-zones/src/error.rs`
- Create: `crates/gnome-zones/src/dbus/mod.rs`
- Create: `crates/gnome-zones/src/dbus/proxy.rs`

The proxy mirrors the daemon's `ZonesInterface` (see `crates/gnome-zones-daemon/src/dbus/interface.rs`) and redeclares the wire types from the daemon's `dbus/types.rs`.

- [ ] **Step 1: Create `src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("D-Bus fdo error: {0}")]
    Fdo(#[from] zbus::fdo::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 2: Create `src/dbus/proxy.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::proxy;
use zbus::zvariant::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ZoneWire {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutSummaryWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MonitorInfoWire {
    pub monitor_key: String,
    pub connector: String,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub is_primary: bool,
}

#[proxy(
    interface = "org.gnome.Zones",
    default_service = "org.gnome.Zones",
    default_path = "/org/gnome/Zones"
)]
pub trait Zones {
    async fn list_layouts(&self) -> zbus::Result<Vec<LayoutSummaryWire>>;
    async fn get_layout(&self, id: i64) -> zbus::Result<LayoutWire>;
    async fn create_layout(&self, name: &str, zones: Vec<ZoneWire>) -> zbus::Result<i64>;
    async fn update_layout(&self, id: i64, name: &str, zones: Vec<ZoneWire>) -> zbus::Result<()>;
    async fn delete_layout(&self, id: i64) -> zbus::Result<()>;

    async fn list_monitors(&self) -> zbus::Result<Vec<MonitorInfoWire>>;
    async fn assign_layout(&self, monitor_key: &str, layout_id: i64) -> zbus::Result<()>;
    async fn get_active_layout(&self, monitor_key: &str) -> zbus::Result<LayoutWire>;

    async fn get_settings(&self) -> zbus::Result<HashMap<String, String>>;
    async fn set_setting(&self, key: &str, value: &str) -> zbus::Result<()>;

    async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> zbus::Result<()>;
    async fn show_activator(&self) -> zbus::Result<()>;
    async fn toggle_paused(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layouts_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layout_assigned(&self, monitor_key: String, layout_id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn monitors_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paused_changed(&self, paused: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activator_requested(&self, monitor_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn editor_requested(&self, monitor_key: String) -> zbus::Result<()>;
}
```

- [ ] **Step 3: Create `src/dbus/mod.rs`**

```rust
pub mod proxy;

use crate::error::Result;
pub use proxy::{LayoutSummaryWire, LayoutWire, MonitorInfoWire, ZoneWire, ZonesProxy};

pub async fn connect() -> Result<ZonesProxy<'static>> {
    let conn = zbus::Connection::session().await?;
    let proxy = ZonesProxy::new(&conn).await?;
    Ok(proxy)
}
```

- [ ] **Step 4: Wire modules into `src/main.rs`**

```rust
mod dbus;
mod error;

fn main() {
    println!("gnome-zones stub");
}
```

- [ ] **Step 5: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success, no warnings about unused imports.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): D-Bus proxy + error types"
```

---

## Task 3: GTK4 application skeleton + CLI

**Files:**
- Create: `crates/gnome-zones/src/app.rs`
- Modify: `crates/gnome-zones/src/main.rs`

The binary supports three modes:
- No args → panel-icon mode (background process, holds indicator, listens for signals)
- `--editor [--monitor <key>]` → open the editor overlay immediately
- `--activator [--monitor <key>]` → open the activator overlay immediately

The CLI flags exist so you can invoke overlays manually (testing / scripting). In normal operation, the daemon fires signals that the panel-icon mode catches.

- [ ] **Step 1: Create `src/app.rs`**

```rust
use clap::Parser;

pub const APP_ID: &str = "org.gnome.Zones";

#[derive(Parser, Debug, Clone)]
#[command(name = "gnome-zones", about = "Zone manager UI for GNOME")]
pub struct Cli {
    /// Open the zone editor overlay and exit when done.
    #[arg(long, conflicts_with = "activator")]
    pub editor: bool,

    /// Open the activator overlay and exit when done.
    #[arg(long, conflicts_with = "editor")]
    pub activator: bool,

    /// Specific monitor_key to target. Defaults to primary monitor.
    #[arg(long)]
    pub monitor: Option<String>,
}

pub fn build_app() -> gtk4::Application {
    libadwaita::init().expect("failed to init libadwaita");
    gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build()
}
```

`NON_UNIQUE` is important: `gnome-zones --activator` invoked from a hotkey script must be able to run while a background `gnome-zones` (panel icon) is already up.

- [ ] **Step 2: Update `src/main.rs`**

```rust
mod app;
mod dbus;
mod error;

use clap::Parser;
use gtk4::prelude::*;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = app::Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let proxy = rt
        .block_on(dbus::connect())
        .expect("failed to connect to org.gnome.Zones — is gnome-zones-daemon running?");

    let application = app::build_app();
    let rt_handle = rt.handle().clone();

    application.connect_activate(move |app| {
        // Keep the runtime alive for the lifetime of the app by stashing
        // the handle on a widget-scope glib data ref (done in later tasks).
        let _ = (app, &proxy, &rt_handle);
        // Task 18 wires up the actual dispatch (panel mode vs. --editor vs. --activator).
        tracing::info!(editor = cli.editor, activator = cli.activator, "gnome-zones launched");
    });

    application.run();
    drop(rt);
}
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Verify runtime (requires daemon running)**

```bash
systemctl --user start gnome-zones-daemon
./target/debug/gnome-zones
```

Expected: process starts, logs `gnome-zones launched`, exits cleanly when you `Ctrl+C`. No window appears yet.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): GTK4 application skeleton + CLI flags"
```

---

## Task 4: Layer-shell helper

**Files:**
- Create: `crates/gnome-zones/src/overlay/mod.rs`
- Create: `crates/gnome-zones/src/overlay/layer_shell.rs`
- Modify: `crates/gnome-zones/src/main.rs`

A reusable helper that creates a borderless, transparent, monitor-filling window on Wayland (via `gtk4-layer-shell`) and falls back to `_NET_WM_WINDOW_TYPE_DOCK` + fullscreen on X11. Both overlays (editor, activator) use it.

- [ ] **Step 1: Create `src/overlay/layer_shell.rs`**

```rust
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Window};
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
///
/// Returns the window without presenting it; the caller sets a child and calls `present()`.
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

    // Transparent background
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
        // X11 fallback: fullscreen on the right monitor, DOCK window type.
        window.fullscreen_on_monitor(monitor);
    }

    window
}

/// Resolve a `gdk::Monitor` for a `monitor_key` produced by the daemon.
/// We match by connector name (the first token of `monitor_key`, e.g. `"DP-1:ab..."`).
/// Returns the primary monitor if no match is found.
pub fn monitor_for_key(display: &gdk::Display, monitor_key: &str) -> gdk::Monitor {
    let connector = monitor_key.split(':').next().unwrap_or("");
    let monitors = display.monitors();
    let n = monitors.n_items();
    for i in 0..n {
        if let Some(m) = monitors.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
            if m.connector().map(|s| s.as_str() == connector).unwrap_or(false) {
                return m;
            }
        }
    }
    // Fall back to the first monitor (GTK4 has no direct "primary" accessor;
    // the daemon's MonitorInfo.is_primary carries that truth).
    monitors
        .item(0)
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
        .expect("no monitors")
}
```

- [ ] **Step 2: Create `src/overlay/mod.rs`**

```rust
pub mod layer_shell;

pub use layer_shell::{build_overlay, monitor_for_key, KeyMode};
```

- [ ] **Step 3: Wire into `main.rs`**

```rust
mod app;
mod dbus;
mod error;
mod overlay;
```

- [ ] **Step 4: Install a global CSS provider for the transparent background**

In `app::build_app`, after `libadwaita::init()`, load the following CSS:

```rust
let provider = gtk4::CssProvider::new();
provider.load_from_string(
    ".gnome-zones-overlay { background: rgba(0, 0, 0, 0); }\n\
     .gnome-zones-editor-backdrop { background: rgba(0, 0, 0, 0.85); }\n\
     .gnome-zones-zone { background: rgba(60, 120, 220, 0.25); \
       border: 2px solid rgba(120, 180, 255, 0.9); border-radius: 4px; }\n\
     .gnome-zones-zone-selected { border: 2px solid rgba(255, 160, 40, 1.0); }\n\
     .gnome-zones-zone-number { color: rgba(255, 255, 255, 0.9); \
       font-size: 96pt; font-weight: bold; }\n",
);
gtk4::style_context_add_provider_for_display(
    &gtk4::gdk::Display::default().expect("no display"),
    &provider,
    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
);
```

Add to the end of `build_app` in `src/app.rs`. Return the application as before.

- [ ] **Step 5: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 6: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): layer-shell overlay helper + global CSS"
```

---

## Task 5: Activator state (pure logic)

**Files:**
- Create: `crates/gnome-zones/src/activator/state.rs`
- Create: `crates/gnome-zones/src/activator/mod.rs`

Pure-Rust input handler that converts a keypress into an action. Unit-tested.

- [ ] **Step 1: Write failing tests**

Create `crates/gnome-zones/src/activator/state.rs`:

```rust
/// Action produced by the activator in response to a keypress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivatorAction {
    /// Snap focused window to `zone_index`; `dismiss` tells the view to close.
    Snap { zone_index: u32, span: bool, dismiss: bool },
    /// Close the overlay without snapping.
    Dismiss,
    /// Ignore the key (no-op; overlay stays open).
    Ignore,
}

/// Compute the action for a given key press.
///
/// * `key_name` — GDK key name (e.g. "1", "KP_1", "Escape", "a").
/// * `shift` — true if Shift is held.
/// * `zone_count` — number of zones in the active layout (keys > zone_count are ignored).
/// * `paused` — if true, only Escape dismisses; digits are ignored.
pub fn handle_key(key_name: &str, shift: bool, zone_count: u32, paused: bool) -> ActivatorAction {
    if key_name == "Escape" {
        return ActivatorAction::Dismiss;
    }
    if paused {
        return ActivatorAction::Ignore;
    }
    let digit = parse_digit(key_name);
    if let Some(d) = digit {
        if d >= 1 && d <= zone_count {
            return ActivatorAction::Snap { zone_index: d, span: shift, dismiss: !shift };
        }
        return ActivatorAction::Ignore;
    }
    // Any other key dismisses (spec: "Any other key | Dismiss without snapping").
    ActivatorAction::Dismiss
}

fn parse_digit(key_name: &str) -> Option<u32> {
    match key_name {
        "1" | "KP_1" => Some(1),
        "2" | "KP_2" => Some(2),
        "3" | "KP_3" => Some(3),
        "4" | "KP_4" => Some(4),
        "5" | "KP_5" => Some(5),
        "6" | "KP_6" => Some(6),
        "7" | "KP_7" => Some(7),
        "8" | "KP_8" => Some(8),
        "9" | "KP_9" => Some(9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_snaps_and_dismisses() {
        assert_eq!(
            handle_key("2", false, 4, false),
            ActivatorAction::Snap { zone_index: 2, span: false, dismiss: true }
        );
    }

    #[test]
    fn shift_digit_snaps_and_stays_open() {
        assert_eq!(
            handle_key("3", true, 4, false),
            ActivatorAction::Snap { zone_index: 3, span: true, dismiss: false }
        );
    }

    #[test]
    fn keypad_digit_accepted() {
        assert_eq!(
            handle_key("KP_5", false, 9, false),
            ActivatorAction::Snap { zone_index: 5, span: false, dismiss: true }
        );
    }

    #[test]
    fn digit_above_zone_count_ignored() {
        assert_eq!(handle_key("5", false, 3, false), ActivatorAction::Ignore);
    }

    #[test]
    fn escape_dismisses() {
        assert_eq!(handle_key("Escape", false, 4, false), ActivatorAction::Dismiss);
    }

    #[test]
    fn other_key_dismisses() {
        assert_eq!(handle_key("a", false, 4, false), ActivatorAction::Dismiss);
    }

    #[test]
    fn paused_ignores_digits_but_escape_works() {
        assert_eq!(handle_key("2", false, 4, true), ActivatorAction::Ignore);
        assert_eq!(handle_key("Escape", false, 4, true), ActivatorAction::Dismiss);
    }
}
```

- [ ] **Step 2: Create `src/activator/mod.rs`**

```rust
pub mod state;

pub use state::{handle_key, ActivatorAction};
```

- [ ] **Step 3: Wire module into `main.rs`**

```rust
mod activator;
mod app;
mod dbus;
mod error;
mod overlay;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones activator::state
```

Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): activator state — key → action mapping"
```

---

## Task 6: Activator view + D-Bus wiring

**Files:**
- Create: `crates/gnome-zones/src/activator/view.rs`
- Modify: `crates/gnome-zones/src/activator/mod.rs`

The view reads the current active layout for the target monitor, renders each zone as a translucent rectangle with a big centered number, and listens for key events. On digit press, it calls `snap_focused_to_zone` via D-Bus.

- [ ] **Step 1: Create `src/activator/view.rs`**

```rust
use crate::activator::state::{handle_key, ActivatorAction};
use crate::dbus::ZonesProxy;
use crate::overlay::{build_overlay, monitor_for_key, KeyMode};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{Align, ApplicationWindow, EventControllerKey, Fixed, Label};
use std::cell::Cell;
use std::rc::Rc;

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
    let display = gdk::Display::default().expect("no display");
    let monitor = monitor_for_key(&display, &monitor_key);

    let window = build_overlay(app, &monitor, KeyMode::OnDemand);
    window.set_title(Some("gnome-zones activator"));

    // Fetch active layout and populate.
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
        let Some(app) = app_weak.upgrade() else { return; };
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

        // Keyboard handling
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

        // Auto-dismiss
        let window_timeout = window.clone();
        gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_millis(ACTIVATOR_TIMEOUT_MS),
            move || { window_timeout.close(); },
        );

        window.present();
    });

    // Present the (empty) window right away so the compositor allocates it;
    // content is added when the async fetch completes.
    window.present();
}

/// Shape the unused import is hiding — explicit.
#[allow(dead_code)]
fn _type_anchor(_: ApplicationWindow) {}
```

- [ ] **Step 2: Update `src/activator/mod.rs`**

```rust
pub mod state;
pub mod view;

pub use state::{handle_key, ActivatorAction};
pub use view::show;
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Manual smoke test**

Make sure the daemon is running and at least one layout is assigned to the primary monitor:

```bash
systemctl --user start gnome-zones-daemon
./target/debug/gnome-zones --activator &
```

Expected: a transparent overlay appears over the primary monitor showing numbered zones. Press `2` — overlay closes; focused window (if any) snaps to zone 2. Press `Esc` — overlay closes without snapping. Overlay auto-dismisses after 3 seconds.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): activator overlay — render zones, handle keys, D-Bus snap"
```

---

## Task 7: Editor state — construction, selection, renumbering

**Files:**
- Create: `crates/gnome-zones/src/editor/state.rs`
- Create: `crates/gnome-zones/src/editor/mod.rs`

Pure-logic module. No GTK.

- [ ] **Step 1: Write failing tests with scaffolding**

Create `crates/gnome-zones/src/editor/state.rs`:

```rust
use crate::dbus::{LayoutWire, ZoneWire};

/// A zone in the working copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<&ZoneWire> for Zone {
    fn from(z: &ZoneWire) -> Self {
        Self { zone_index: z.zone_index, x: z.x, y: z.y, w: z.w, h: z.h }
    }
}

impl From<&Zone> for ZoneWire {
    fn from(z: &Zone) -> Self {
        ZoneWire { zone_index: z.zone_index, x: z.x, y: z.y, w: z.w, h: z.h }
    }
}

/// Editor working copy.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub layout_id: Option<i64>,       // None = brand-new unsaved layout
    pub name: String,
    pub is_preset: bool,              // true = source was a preset (read-only)
    pub zones: Vec<Zone>,
    pub selected: Option<u32>,        // zone_index of current selection
    original: Vec<Zone>,              // for reset() and is_dirty()
    original_name: String,
}

impl EditorState {
    pub fn from_layout(layout: &LayoutWire) -> Self {
        let zones: Vec<Zone> = layout.zones.iter().map(Zone::from).collect();
        Self {
            layout_id: Some(layout.id),
            name: layout.name.clone(),
            is_preset: layout.is_preset,
            zones: zones.clone(),
            selected: zones.first().map(|z| z.zone_index),
            original: zones,
            original_name: layout.name.clone(),
        }
    }

    pub fn select(&mut self, zone_index: u32) {
        if self.zones.iter().any(|z| z.zone_index == zone_index) {
            self.selected = Some(zone_index);
        }
    }

    pub fn selected_zone(&self) -> Option<&Zone> {
        self.selected.and_then(|i| self.zones.iter().find(|z| z.zone_index == i))
    }

    pub fn reset(&mut self) {
        self.zones = self.original.clone();
        self.name = self.original_name.clone();
        self.selected = self.zones.first().map(|z| z.zone_index);
    }

    pub fn is_dirty(&self) -> bool {
        self.name != self.original_name || self.zones != self.original
    }

    /// Renumber zones in row-major reading order based on top-left corner.
    /// Preserves `selected` by tracking the zone identity through the resort.
    pub fn renumber_row_major(&mut self) {
        let selected_pos = self
            .selected
            .and_then(|i| self.zones.iter().position(|z| z.zone_index == i));

        // Sort stable by (y, x) with a small epsilon so zones that share a row
        // always compare left-to-right.
        let eps = 1e-6;
        let mut order: Vec<usize> = (0..self.zones.len()).collect();
        order.sort_by(|&a, &b| {
            let za = &self.zones[a];
            let zb = &self.zones[b];
            if (za.y - zb.y).abs() < eps {
                za.x.partial_cmp(&zb.x).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                za.y.partial_cmp(&zb.y).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        let reordered: Vec<Zone> = order
            .iter()
            .enumerate()
            .map(|(new_idx, &old_pos)| {
                let mut z = self.zones[old_pos];
                z.zone_index = (new_idx + 1) as u32;
                z
            })
            .collect();

        // Update selection to the new index of the previously-selected zone.
        if let Some(old_pos) = selected_pos {
            let new_pos = order.iter().position(|&p| p == old_pos).unwrap();
            self.selected = Some((new_pos + 1) as u32);
        }
        self.zones = reordered;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::{LayoutWire, ZoneWire};

    fn zw(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneWire {
        ZoneWire { zone_index: i, x, y, w, h }
    }

    fn two_col_layout() -> LayoutWire {
        LayoutWire {
            id: 7,
            name: "Two Columns".into(),
            is_preset: true,
            zones: vec![zw(1, 0.0, 0.0, 0.5, 1.0), zw(2, 0.5, 0.0, 0.5, 1.0)],
        }
    }

    #[test]
    fn from_layout_seeds_selection() {
        let s = EditorState::from_layout(&two_col_layout());
        assert_eq!(s.selected, Some(1));
        assert_eq!(s.zones.len(), 2);
        assert!(s.is_preset);
        assert!(!s.is_dirty());
    }

    #[test]
    fn select_existing_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(2);
        assert_eq!(s.selected, Some(2));
    }

    #[test]
    fn select_ignores_unknown_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(99);
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn renumber_sorts_row_major() {
        // Bottom-right then top-left — renumber must produce 1=TL, 2=BR.
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "tmp".into(), is_preset: false,
            zones: vec![
                zw(1, 0.5, 0.5, 0.5, 0.5),
                zw(2, 0.0, 0.0, 0.5, 0.5),
            ],
        });
        s.select(2); // top-left
        s.renumber_row_major();
        assert_eq!(s.zones[0].zone_index, 1);
        assert!((s.zones[0].x - 0.0).abs() < 1e-9);
        assert!((s.zones[0].y - 0.0).abs() < 1e-9);
        assert_eq!(s.zones[1].zone_index, 2);
        // Selection identity preserved — still on the top-left zone (now index 1)
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn renumber_groups_by_row() {
        // Two rows of two: (TL, TR, BL, BR).
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "tmp".into(), is_preset: false,
            zones: vec![
                zw(1, 0.5, 0.5, 0.5, 0.5),  // BR
                zw(2, 0.0, 0.5, 0.5, 0.5),  // BL
                zw(3, 0.5, 0.0, 0.5, 0.5),  // TR
                zw(4, 0.0, 0.0, 0.5, 0.5),  // TL
            ],
        });
        s.renumber_row_major();
        assert_eq!(s.zones[0].x, 0.0); assert_eq!(s.zones[0].y, 0.0); // TL = 1
        assert_eq!(s.zones[1].x, 0.5); assert_eq!(s.zones[1].y, 0.0); // TR = 2
        assert_eq!(s.zones[2].x, 0.0); assert_eq!(s.zones[2].y, 0.5); // BL = 3
        assert_eq!(s.zones[3].x, 0.5); assert_eq!(s.zones[3].y, 0.5); // BR = 4
    }

    #[test]
    fn reset_restores_original() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.name = "garbage".into();
        s.zones[0].w = 0.9;
        assert!(s.is_dirty());
        s.reset();
        assert!(!s.is_dirty());
        assert_eq!(s.name, "Two Columns");
        assert_eq!(s.zones[0].w, 0.5);
    }
}
```

- [ ] **Step 2: Create `src/editor/mod.rs`**

```rust
pub mod state;

pub use state::{EditorState, Zone};
```

- [ ] **Step 3: Wire module into `main.rs`**

```rust
mod activator;
mod app;
mod dbus;
mod editor;
mod error;
mod overlay;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones editor::state
```

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): editor state — construct, select, renumber, reset"
```

---

## Task 8: Editor state — split horizontal / vertical

**Files:**
- Modify: `crates/gnome-zones/src/editor/state.rs`

Splits the currently-selected zone into two zones of equal size, stacked (horizontal split → top/bottom) or side-by-side (vertical split → left/right). Renumbers row-major immediately. Selection moves to the first of the two new zones.

- [ ] **Step 1: Write failing tests**

Append to the `#[cfg(test)] mod tests` block in `state.rs`:

```rust
    #[test]
    fn split_horizontal_selected_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(1);  // left column
        s.split_horizontal();
        assert_eq!(s.zones.len(), 3);
        // Left column replaced by top+bottom halves
        let top = s.zones.iter().find(|z| z.x == 0.0 && z.y == 0.0).unwrap();
        let bot = s.zones.iter().find(|z| z.x == 0.0 && (z.y - 0.5).abs() < 1e-9).unwrap();
        assert!((top.h - 0.5).abs() < 1e-9);
        assert!((bot.h - 0.5).abs() < 1e-9);
        assert!((top.w - 0.5).abs() < 1e-9);
        // Selection lands on the first new zone (top half, now index 1 after renumber)
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn split_vertical_selected_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(1);  // left column (0..0.5 × 0..1)
        s.split_vertical();
        assert_eq!(s.zones.len(), 3);
        let left = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let mid  = s.zones.iter().find(|z| (z.x - 0.25).abs() < 1e-9).unwrap();
        assert!((left.w - 0.25).abs() < 1e-9);
        assert!((mid.w  - 0.25).abs() < 1e-9);
        assert!((left.h - 1.0).abs()  < 1e-9);
    }

    #[test]
    fn split_without_selection_is_noop() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.selected = None;
        s.split_horizontal();
        assert_eq!(s.zones.len(), 2);
    }

    #[test]
    fn split_marks_dirty() {
        let mut s = EditorState::from_layout(&two_col_layout());
        assert!(!s.is_dirty());
        s.split_vertical();
        assert!(s.is_dirty());
    }
```

- [ ] **Step 2: Run the new tests — expect compile failure (methods don't exist)**

```bash
cargo test -p gnome-zones editor::state::tests::split_horizontal_selected_zone
```

Expected: "no method named `split_horizontal` found".

- [ ] **Step 3: Implement the methods**

Add to `impl EditorState` in `state.rs`:

```rust
    /// Split the selected zone into top/bottom halves. No-op if nothing selected.
    pub fn split_horizontal(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };

        let z = self.zones[pos];
        let half = z.h / 2.0;
        let top = Zone { zone_index: 0, x: z.x, y: z.y,          w: z.w, h: half };
        let bot = Zone { zone_index: 0, x: z.x, y: z.y + half,   w: z.w, h: half };

        self.zones.remove(pos);
        self.zones.push(top);
        self.zones.push(bot);
        self.renumber_row_major();
        // After renumber, set selection to whichever zone now sits at (top.x, top.y).
        if let Some(t) = self.zones.iter().find(|zz| (zz.x - top.x).abs() < 1e-9 && (zz.y - top.y).abs() < 1e-9) {
            self.selected = Some(t.zone_index);
        }
    }

    /// Split the selected zone into left/right halves. No-op if nothing selected.
    pub fn split_vertical(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };

        let z = self.zones[pos];
        let half = z.w / 2.0;
        let left  = Zone { zone_index: 0, x: z.x,         y: z.y, w: half, h: z.h };
        let right = Zone { zone_index: 0, x: z.x + half,  y: z.y, w: half, h: z.h };

        self.zones.remove(pos);
        self.zones.push(left);
        self.zones.push(right);
        self.renumber_row_major();
        if let Some(l) = self.zones.iter().find(|zz| (zz.x - left.x).abs() < 1e-9 && (zz.y - left.y).abs() < 1e-9) {
            self.selected = Some(l.zone_index);
        }
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones editor::state
```

Expected: 10 tests pass (6 from Task 7 + 4 new).

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/editor/state.rs
git commit -m "feat(zones-ui): editor state — split horizontal / vertical"
```

---

## Task 9: Editor state — delete with edge-merge

**Files:**
- Modify: `crates/gnome-zones/src/editor/state.rs`

Delete the selected zone. If a single neighbor zone shares the entire deleted zone's edge (left/right/top/bottom), extend that neighbor to absorb the deleted area. If multiple such neighbors exist, pick the one with the largest area. If none match exactly, just remove the zone (leaves an unzoned hole).

- [ ] **Step 1: Write failing tests**

Append to the test module:

```rust
    #[test]
    fn delete_extends_neighbor_when_edges_match() {
        // Two columns (0..0.5 | 0.5..1.0), delete right → left extends to cover full width.
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(2);
        s.delete_selected();
        assert_eq!(s.zones.len(), 1);
        let z = &s.zones[0];
        assert!((z.x - 0.0).abs() < 1e-9);
        assert!((z.w - 1.0).abs() < 1e-9);
        assert!((z.h - 1.0).abs() < 1e-9);
        assert_eq!(z.zone_index, 1);
    }

    #[test]
    fn delete_picks_largest_neighbor_on_tie() {
        // Layout: top half is one zone; bottom half split into two.
        // Delete the large top zone — no single neighbor spans its width,
        // so it should just disappear (no merge).
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                zw(1, 0.0, 0.0, 1.0, 0.5),  // top full width
                zw(2, 0.0, 0.5, 0.5, 0.5),  // bottom-left
                zw(3, 0.5, 0.5, 0.5, 0.5),  // bottom-right
            ],
        });
        s.select(1);
        s.delete_selected();
        assert_eq!(s.zones.len(), 2);  // two bottom zones remain, top is gone
    }

    #[test]
    fn delete_last_zone_leaves_empty_layout() {
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![zw(1, 0.0, 0.0, 1.0, 1.0)],
        });
        s.select(1);
        s.delete_selected();
        assert!(s.zones.is_empty());
        assert_eq!(s.selected, None);
    }

    #[test]
    fn delete_without_selection_is_noop() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.selected = None;
        s.delete_selected();
        assert_eq!(s.zones.len(), 2);
    }
```

- [ ] **Step 2: Run tests — expect compile failure**

```bash
cargo test -p gnome-zones editor::state::tests::delete_extends_neighbor_when_edges_match
```

Expected: "no method named `delete_selected` found".

- [ ] **Step 3: Implement `delete_selected`**

Add to `impl EditorState`:

```rust
    /// Delete the selected zone. If a single neighbor shares the full edge
    /// where the deletion happens, extend it to cover the deleted area.
    /// Otherwise just remove. Always renumbers row-major afterward.
    pub fn delete_selected(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };
        let deleted = self.zones[pos];

        // Find candidate neighbors sharing a full edge with `deleted`.
        // Left edge of deleted  = right edge of neighbor; widths sum seamlessly.
        let eps = 1e-6;
        let mut candidates: Vec<(usize, f64)> = Vec::new();  // (idx, neighbor_area)

        for (i, n) in self.zones.iter().enumerate() {
            if i == pos { continue; }
            // Neighbor sits to the LEFT of deleted, sharing its right edge with deleted's left edge,
            // and spans the full vertical extent of deleted.
            let right_matches = (n.x + n.w - deleted.x).abs() < eps
                && (n.y - deleted.y).abs() < eps
                && (n.h - deleted.h).abs() < eps;
            let left_matches  = (n.x - (deleted.x + deleted.w)).abs() < eps
                && (n.y - deleted.y).abs() < eps
                && (n.h - deleted.h).abs() < eps;
            let below_matches = (n.y - (deleted.y + deleted.h)).abs() < eps
                && (n.x - deleted.x).abs() < eps
                && (n.w - deleted.w).abs() < eps;
            let above_matches = (n.y + n.h - deleted.y).abs() < eps
                && (n.x - deleted.x).abs() < eps
                && (n.w - deleted.w).abs() < eps;

            if right_matches || left_matches || above_matches || below_matches {
                candidates.push((i, n.w * n.h));
            }
        }

        // Pick largest neighbor by area; ties broken by earliest index.
        let chosen = candidates
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((ni, _)) = chosen {
            // Compute adjusted neighbor (absorbing deleted).
            let n = self.zones[ni];
            let merged = merge_rects(&n, &deleted);
            self.zones[ni] = merged;
            // Remove deleted (recompute index after possible shift — ni < pos or > pos).
            let pos_after = if ni < pos { pos } else { pos };
            self.zones.remove(pos_after);
        } else {
            self.zones.remove(pos);
        }

        if self.zones.is_empty() {
            self.selected = None;
        } else {
            self.renumber_row_major();
            self.selected = Some(self.zones[0].zone_index);
        }
    }

// Extend the merged neighbor to cover the union bounding box (valid because
// the caller has already verified the edge match).
fn merge_rects(neighbor: &Zone, deleted: &Zone) -> Zone {
    let x0 = neighbor.x.min(deleted.x);
    let y0 = neighbor.y.min(deleted.y);
    let x1 = (neighbor.x + neighbor.w).max(deleted.x + deleted.w);
    let y1 = (neighbor.y + neighbor.h).max(deleted.y + deleted.h);
    Zone {
        zone_index: neighbor.zone_index,
        x: x0, y: y0,
        w: x1 - x0, h: y1 - y0,
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones editor::state
```

Expected: 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/editor/state.rs
git commit -m "feat(zones-ui): editor state — delete with edge-merge"
```

---

## Task 10: Editor state — add_zone + move_divider

**Files:**
- Modify: `crates/gnome-zones/src/editor/state.rs`

Two more primitive operations for view-driven edits:
- `add_zone` — append a user-drawn zone (from click-drag on empty space).
- `move_divider` — given two zones sharing a divider edge and a fractional delta, resize both.

- [ ] **Step 1: Write failing tests**

Append to the test module:

```rust
    #[test]
    fn add_zone_appends_and_renumbers() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.add_zone(0.1, 0.1, 0.2, 0.2);
        assert_eq!(s.zones.len(), 3);
        // New zone (x=0.1, y=0.1) renumbers ahead of the old bottom zones in row-major order.
        let new_zone = s.zones.iter().find(|z| (z.x - 0.1).abs() < 1e-9).unwrap();
        assert!((new_zone.w - 0.2).abs() < 1e-9);
        assert!(s.is_dirty());
    }

    #[test]
    fn move_divider_vertical_between_columns() {
        let mut s = EditorState::from_layout(&two_col_layout());
        // Move the vertical divider at x=0.5 to x=0.4 — left shrinks, right grows.
        s.move_divider(1, 2, Axis::Vertical, -0.1);
        let left  = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let right = s.zones.iter().find(|z| (z.x - 0.4).abs() < 1e-9).unwrap();
        assert!((left.w - 0.4).abs() < 1e-9);
        assert!((right.w - 0.6).abs() < 1e-9);
    }

    #[test]
    fn move_divider_clamps_at_edges() {
        let mut s = EditorState::from_layout(&two_col_layout());
        // Try to move divider past the right edge — both widths must stay > 0.
        s.move_divider(1, 2, Axis::Vertical, 0.6);
        let left  = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let right = s.zones.iter().find(|z| z.x > 0.0).unwrap();
        assert!(left.w > 0.0);
        assert!(right.w > 0.0);
        assert!((left.w + right.w - 1.0).abs() < 1e-9);
    }
```

- [ ] **Step 2: Run tests — expect compile failure**

```bash
cargo test -p gnome-zones editor::state::tests::add_zone_appends_and_renumbers
```

Expected: `no method named add_zone` + `no variant Axis`.

- [ ] **Step 3: Implement methods and Axis enum**

Add to `state.rs` above the `impl EditorState` block:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Divider runs vertically (left zone sits to the left of right zone).
    Vertical,
    /// Divider runs horizontally (top zone sits above bottom zone).
    Horizontal,
}
```

Add to `impl EditorState`:

```rust
    /// Append a user-drawn zone. Fractional coords; renumbers row-major.
    pub fn add_zone(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.zones.push(Zone { zone_index: 0, x, y, w, h });
        self.renumber_row_major();
    }

    /// Move a shared divider between two zones by a fractional delta.
    /// * `first_idx` — zone_index of the zone on the "smaller-coord" side of the divider.
    /// * `second_idx` — zone_index of the zone on the "larger-coord" side.
    /// * `axis` — orientation of the divider itself.
    /// * `delta` — signed fractional offset: positive shrinks `first`, grows `second` for
    ///   Vertical; positive shrinks `first`, grows `second` for Horizontal (i.e. the
    ///   divider moves in the +x or +y direction).
    ///
    /// Clamps so neither zone collapses below `MIN_DIVIDER_GAP`.
    pub fn move_divider(&mut self, first_idx: u32, second_idx: u32, axis: Axis, delta: f64) {
        const MIN_DIVIDER_GAP: f64 = 0.02;
        let Some(pa) = self.zones.iter().position(|z| z.zone_index == first_idx) else { return; };
        let Some(pb) = self.zones.iter().position(|z| z.zone_index == second_idx) else { return; };
        if pa == pb { return; }

        match axis {
            Axis::Vertical => {
                let a_w = self.zones[pa].w;
                let b_x = self.zones[pb].x;
                let b_w = self.zones[pb].w;
                let d = delta
                    .max(-a_w + MIN_DIVIDER_GAP)
                    .min( b_w - MIN_DIVIDER_GAP);
                self.zones[pa].w = a_w + d;
                self.zones[pb].x = b_x + d;
                self.zones[pb].w = b_w - d;
            }
            Axis::Horizontal => {
                let a_h = self.zones[pa].h;
                let b_y = self.zones[pb].y;
                let b_h = self.zones[pb].h;
                let d = delta
                    .max(-a_h + MIN_DIVIDER_GAP)
                    .min( b_h - MIN_DIVIDER_GAP);
                self.zones[pa].h = a_h + d;
                self.zones[pb].y = b_y + d;
                self.zones[pb].h = b_h - d;
            }
        }
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p gnome-zones editor::state
```

Expected: 17 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/editor/state.rs
git commit -m "feat(zones-ui): editor state — add_zone + move_divider with clamping"
```

---

## Task 11: Editor view — render backdrop, zones, and big numbers

**Files:**
- Create: `crates/gnome-zones/src/editor/view.rs`
- Modify: `crates/gnome-zones/src/editor/mod.rs`

Renders the `EditorState` to GTK. Selection, toolbar, and dragging come in later tasks — this task produces a visible but inert editor.

- [ ] **Step 1: Create `src/editor/view.rs`**

```rust
use crate::dbus::{LayoutSummaryWire, LayoutWire, ZonesProxy};
use crate::editor::state::{EditorState, Zone};
use crate::overlay::{build_overlay, monitor_for_key, KeyMode};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GBox, Fixed, GestureClick, Label, Orientation,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct EditorView {
    pub window: ApplicationWindow,
    pub canvas: Fixed,
    pub toolbar_container: GBox,  // set up in Task 12
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
        _all_layouts: Vec<LayoutSummaryWire>,
    ) -> Rc<Self> {
        let display = gdk::Display::default().expect("no display");
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

        // Toolbar placeholder — populated in Task 12
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

        view.rerender();
        view
    }

    /// Rebuild all zone rectangles from scratch. Cheap enough for v1 — layouts have < 20 zones.
    pub fn rerender(self: &Rc<Self>) {
        // Remove previous widgets
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

        // Click to select
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
}
```

- [ ] **Step 2: Update `src/editor/mod.rs`**

```rust
pub mod state;
pub(crate) mod view;

pub use state::{Axis, EditorState, Zone};
pub use view::EditorView;
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): editor view — render zones and click-to-select"
```

---

## Task 12: Editor view — toolbar

**Files:**
- Modify: `crates/gnome-zones/src/editor/view.rs`

The toolbar contains the layout dropdown, `+ New from current`, `Save as…`, `Reset`, split/delete buttons, a gap spinner, and Apply / Cancel buttons. Actions wire into the state; persistence happens in Task 15.

- [ ] **Step 1: Add toolbar-building method**

Append to `impl EditorView` in `view.rs`:

```rust
    /// Build the bottom toolbar — called once from `new`.
    pub(crate) fn build_toolbar(self: &Rc<Self>, all_layouts: &[LayoutSummaryWire]) {
        use gtk4::{Button, DropDown, SpinButton, StringList};

        // Layout dropdown
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
        let saveas    = Button::with_label("Save as…");
        let reset     = Button::with_label("Reset");
        let split_h   = Button::with_label("➕ Split horizontal");
        let split_v   = Button::with_label("➕ Split vertical");
        let del       = Button::with_label("🗑 Delete");
        let gap_spin  = SpinButton::with_range(0.0, 64.0, 1.0);
        gap_spin.set_value(8.0);
        let apply_btn = Button::with_label("✓ Apply");
        apply_btn.add_css_class("suggested-action");
        let cancel    = Button::with_label("✕ Cancel");

        for w in [&new_btn, &saveas, &reset, &split_h, &split_v, &del, &gap_spin.clone().upcast(), &apply_btn, &cancel] {
            self.toolbar_container.append(w);
        }

        // Split / delete wiring (Task 12 — pure state mutations + rerender)
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

        // Apply / Cancel / Reset / New / Save as / dropdown wiring — Task 15.
        let _ = (new_btn, saveas, reset, gap_spin, apply_btn, cancel, dropdown);
    }
```

- [ ] **Step 2: Call `build_toolbar` from `EditorView::new`**

Replace the `view.rerender();` line near the end of `new()` with:

```rust
        view.build_toolbar(&_all_layouts);
        view.rerender();
```

Rename the parameter from `_all_layouts` to `all_layouts` now that it's used.

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones/src/editor/view.rs
git commit -m "feat(zones-ui): editor toolbar + split/delete wiring"
```

---

## Task 13: Editor view — divider drag-to-resize

**Files:**
- Modify: `crates/gnome-zones/src/editor/view.rs`

Adds drag handles for each shared divider. The divider widget is a thin draggable strip positioned on the canvas between two adjacent zones. During drag, it calls `state.move_divider(...)` and triggers a rerender.

For v1 simplicity: scan all zone pairs at rerender time, find pairs sharing a full edge (same detection as `delete_selected`'s edge-merge), and place a 6px-wide handle on that edge.

- [ ] **Step 1: Add divider-detection helper to state**

Append to `crates/gnome-zones/src/editor/state.rs` inside `impl EditorState`:

```rust
    /// Return all divider pairs (first_idx, second_idx, axis) where the two zones share
    /// a full edge. Used by the view to place drag handles.
    pub fn shared_edges(&self) -> Vec<(u32, u32, Axis)> {
        let eps = 1e-6;
        let mut out = Vec::new();
        for (i, a) in self.zones.iter().enumerate() {
            for b in self.zones.iter().skip(i + 1) {
                // a's right edge meets b's left edge, same vertical span
                if (a.x + a.w - b.x).abs() < eps
                    && (a.y - b.y).abs() < eps
                    && (a.h - b.h).abs() < eps {
                    out.push((a.zone_index, b.zone_index, Axis::Vertical));
                }
                // a's left edge meets b's right edge
                else if (b.x + b.w - a.x).abs() < eps
                    && (a.y - b.y).abs() < eps
                    && (a.h - b.h).abs() < eps {
                    out.push((b.zone_index, a.zone_index, Axis::Vertical));
                }
                // a's bottom edge meets b's top edge
                else if (a.y + a.h - b.y).abs() < eps
                    && (a.x - b.x).abs() < eps
                    && (a.w - b.w).abs() < eps {
                    out.push((a.zone_index, b.zone_index, Axis::Horizontal));
                }
                // a's top edge meets b's bottom edge
                else if (b.y + b.h - a.y).abs() < eps
                    && (a.x - b.x).abs() < eps
                    && (a.w - b.w).abs() < eps {
                    out.push((b.zone_index, a.zone_index, Axis::Horizontal));
                }
            }
        }
        out
    }
```

- [ ] **Step 2: Write a test for `shared_edges`**

Append to the tests module:

```rust
    #[test]
    fn shared_edges_for_two_columns() {
        let s = EditorState::from_layout(&two_col_layout());
        let edges = s.shared_edges();
        assert_eq!(edges.len(), 1);
        let (a, b, axis) = edges[0];
        assert_eq!((a, b), (1, 2));
        assert_eq!(axis, Axis::Vertical);
    }

    #[test]
    fn shared_edges_for_2x2() {
        let s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                zw(1, 0.0, 0.0, 0.5, 0.5),
                zw(2, 0.5, 0.0, 0.5, 0.5),
                zw(3, 0.0, 0.5, 0.5, 0.5),
                zw(4, 0.5, 0.5, 0.5, 0.5),
            ],
        });
        let edges = s.shared_edges();
        // 4 shared edges: top pair, bottom pair, left pair, right pair
        assert_eq!(edges.len(), 4);
    }
```

Run tests:

```bash
cargo test -p gnome-zones editor::state
```

Expected: 19 tests pass.

- [ ] **Step 3: Render divider handles in the view**

Add to `impl EditorView` in `view.rs`:

```rust
    fn build_divider_handles(self: &Rc<Self>) {
        use gtk4::GestureDrag;

        const HANDLE_THICKNESS: i32 = 6;
        let edges = self.state.borrow().shared_edges();
        for (first_idx, second_idx, axis) in edges {
            let state = self.state.borrow();
            let a = state.zones.iter().find(|z| z.zone_index == first_idx).unwrap();
            let b = state.zones.iter().find(|z| z.zone_index == second_idx).unwrap();

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
            drop(state);
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
```

- [ ] **Step 4: Add CSS for the divider handle**

In `src/app.rs` inside the CSS string, add:

```css
.gnome-zones-divider { background: rgba(255, 255, 255, 0.4); border-radius: 3px; }
```

- [ ] **Step 5: Call `build_divider_handles` from `rerender`**

At the end of `EditorView::rerender`, after the zone loop, add:

```rust
        drop(state);
        self.build_divider_handles();
```

- [ ] **Step 6: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 7: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): editor — draggable divider handles"
```

---

## Task 14: Editor view — click-drag to draw new zone

**Files:**
- Modify: `crates/gnome-zones/src/editor/view.rs`

A `GestureDrag` attached to the canvas itself (not to any zone). On drag start, check that the starting point is *not* inside any existing zone. On drag update, render a live "ghost" rectangle showing the proposed new zone. On drag end, call `state.add_zone(...)` and rerender.

- [ ] **Step 1: Add canvas-level drag handler**

Append to `impl EditorView` in `view.rs`:

```rust
    fn wire_canvas_drag(self: &Rc<Self>) {
        use gtk4::GestureDrag;

        let drag = GestureDrag::new();
        let view_w = Rc::downgrade(self);

        // Live "ghost" preview
        let ghost: Rc<RefCell<Option<gtk4::Widget>>> = Rc::new(RefCell::new(None));

        {
            let ghost = ghost.clone();
            let view_w = view_w.clone();
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
                *ghost.borrow_mut() = Some(g.upcast());
            });
        }

        {
            let ghost = ghost.clone();
            let view_w = view_w.clone();
            drag.connect_drag_update(move |g, dx, dy| {
                let Some(view) = view_w.upgrade() else { return; };
                let Some(w) = ghost.borrow().clone() else { return; };
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
            let view_w = view_w.clone();
            drag.connect_drag_end(move |g, dx, dy| {
                let Some(view) = view_w.upgrade() else { return; };
                if let Some(w) = ghost.borrow_mut().take() {
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
```

- [ ] **Step 2: Call `wire_canvas_drag` from `EditorView::new`**

After `view.build_toolbar(&all_layouts);` and before `view.rerender();`, add:

```rust
        view.wire_canvas_drag();
```

- [ ] **Step 3: Add CSS for the ghost rectangle**

In `src/app.rs` CSS string:

```css
.gnome-zones-zone-ghost { background: rgba(255, 160, 40, 0.3); border: 2px dashed rgba(255, 160, 40, 0.9); border-radius: 4px; }
```

- [ ] **Step 4: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): editor — click-drag to draw new zone"
```

---

## Task 15: Editor view — Apply / Cancel / Reset / New / Save as

**Files:**
- Modify: `crates/gnome-zones/src/editor/view.rs`

Wire up the persistence actions. `Apply` pushes the edited zones back to the daemon via `UpdateLayout` (or `CreateLayout` if `layout_id = None`) and then `AssignLayout` for the current monitor. `Save as…` prompts for a name and forks. `+ New from current` clears `layout_id` and renames to "Untitled". `Reset` calls `state.reset()`. `Cancel` closes the window.

- [ ] **Step 1: Replace the stub at the bottom of `build_toolbar`**

In `view.rs`, delete the trailing line:

```rust
        let _ = (new_btn, saveas, reset, gap_spin, apply_btn, cancel, dropdown);
```

Replace with the following:

```rust
        // Gap spinner → daemon setting
        {
            let proxy = self.proxy.clone();
            gap_spin.connect_value_changed(move |sb| {
                let value = sb.value_as_int().to_string();
                let proxy = proxy.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    let _ = proxy.set_setting("gap_px", &value).await;
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

        // New from current — clears layout_id, renames
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
                    if let Ok(layout) = proxy.get_layout(id).await {
                        if let Some(v) = view_.upgrade() {
                            *v.state.borrow_mut() = EditorState::from_layout(&layout);
                            v.rerender();
                        }
                    }
                });
            });
        }
```

- [ ] **Step 2: Implement `apply_and_close` and `show_save_as_dialog`**

Append to `impl EditorView`:

```rust
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
                        Err(e) => { tracing::warn!(error = %e, "apply: create failed"); return; }
                    }
                } else {
                    if let Err(e) = proxy.update_layout(id, &state.name, zones).await {
                        tracing::warn!(error = %e, "apply: update failed"); return;
                    }
                    id
                }
            } else {
                match proxy.create_layout(&state.name, zones).await {
                    Ok(id) => id,
                    Err(e) => { tracing::warn!(error = %e, "apply: create failed"); return; }
                }
            };
            if let Err(e) = proxy.assign_layout(&monitor_key, id).await {
                tracing::warn!(error = %e, "apply: assign failed");
            }
            window.close();
        });
    }

    fn show_save_as_dialog(self: &Rc<Self>) {
        use libadwaita::prelude::*;
        use libadwaita::{MessageDialog, ResponseAppearance};

        let dialog = MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .heading("Save layout as")
            .body("Enter a name for the new layout.")
            .build();

        let entry = gtk4::Entry::builder()
            .text(&format!("{} (copy)", self.state.borrow().name))
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));

        let proxy = self.proxy.clone();
        let view = Rc::downgrade(self);
        dialog.connect_response(None, move |dialog, response| {
            if response == "save" {
                let name = entry.text().to_string();
                let Some(v) = view.upgrade() else { dialog.close(); return; };
                let zones: Vec<_> = v.state.borrow().zones.iter().map(Into::into).collect();
                let proxy = proxy.clone();
                let view_ = view.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    if let Ok(id) = proxy.create_layout(&name, zones).await {
                        if let Some(v) = view_.upgrade() {
                            // Reload from the new layout so is_preset=false and id is set.
                            if let Ok(layout) = proxy.get_layout(id).await {
                                *v.state.borrow_mut() = EditorState::from_layout(&layout);
                                v.rerender();
                            }
                        }
                    }
                });
            }
            dialog.close();
        });
        dialog.present();
    }
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones/src/editor/view.rs
git commit -m "feat(zones-ui): editor — Apply/Cancel/Reset/New/Save-as/gap wired to D-Bus"
```

---

## Task 16: Editor — public `show` entry point + signal handler skeleton

**Files:**
- Modify: `crates/gnome-zones/src/editor/view.rs`
- Modify: `crates/gnome-zones/src/editor/mod.rs`

Provide a single `editor::show(app, proxy, monitor_key)` function that fetches layouts + the active layout for the monitor and presents the editor.

- [ ] **Step 1: Add `show` function to `view.rs`**

Append to `view.rs` (outside `impl EditorView`):

```rust
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
            Err(e) => { tracing::warn!(error = %e, "editor: list_layouts failed"); return; }
        };
        let active = match proxy_fetch.get_active_layout(&mk).await {
            Ok(l) => l,
            Err(e) => { tracing::warn!(error = %e, "editor: get_active_layout failed"); return; }
        };
        let Some(app) = app_weak.upgrade() else { return; };
        let view = EditorView::new(&app, proxy, mk, active, layouts);
        view.window.present();
    });
}
```

- [ ] **Step 2: Update `src/editor/mod.rs`**

```rust
pub mod state;
pub(crate) mod view;

pub use state::{Axis, EditorState, Zone};
pub use view::{show, EditorView};
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 4: Manual smoke test**

```bash
./target/debug/gnome-zones --editor
```

Expected: a full-screen editor appears over the primary monitor with the current layout's zones as blue rectangles and the toolbar at the bottom. Clicking a zone selects it (orange outline). Split / delete buttons work. Divider handles resize. Click-drag in empty space draws a new zone. Apply persists and closes; Cancel closes without saving.

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): editor public show entry point"
```

---

## Task 17: Panel status icon

**Files:**
- Create: `crates/gnome-zones/src/panel/mod.rs`
- Create: `crates/gnome-zones/src/panel/indicator.rs`
- Modify: `crates/gnome-zones/src/main.rs`

Panel icon via `libayatana-appindicator`. Left-click triggers `ShowActivator` (daemon emits signal; UI catches it). Right-click menu contains layout switching, Edit zones…, Pause/Resume, About.

- [ ] **Step 1: Create `src/panel/indicator.rs`**

```rust
use crate::dbus::{LayoutSummaryWire, ZonesProxy};
use gtk4::prelude::*;
use libayatana_appindicator::{AppIndicator, AppIndicatorStatus};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Indicator {
    inner: RefCell<AppIndicator>,
    menu: gtk4::Menu,
}

impl Indicator {
    pub fn new(
        proxy: ZonesProxy<'static>,
        layouts: Vec<LayoutSummaryWire>,
        paused: bool,
    ) -> Rc<Self> {
        let mut ind = AppIndicator::new("gnome-zones", "");
        ind.set_status(AppIndicatorStatus::Active);
        ind.set_icon_full(if paused { "view-grid-symbolic" } else { "view-grid-symbolic" }, "gnome-zones");

        let menu = gtk4::Menu::new();

        // Left-click — but appindicator only supports right-click menu; for left-click
        // we use "secondary-activate-target" set to a specific item, or just treat
        // left-click as opening the activator via a dedicated item.
        let activator_item = gtk4::MenuItem::with_label("Show activator");
        {
            let proxy = proxy.clone();
            activator_item.connect_activate(move |_| {
                let proxy = proxy.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    let _ = proxy.show_activator().await;
                });
            });
        }
        menu.append(&activator_item);

        let edit_item = gtk4::MenuItem::with_label("Edit zones…");
        {
            // Fire EditorRequested by calling a method — we don't have one; instead
            // we emit a local in-process trigger via glib action. Simpler: hit the
            // same D-Bus path as the Super+Shift+E hotkey by having the daemon expose
            // a method. For now, use the direct editor::show() call from main.rs via
            // a channel. Implemented in Task 18.
            edit_item.set_sensitive(true);
        }
        menu.append(&edit_item);

        menu.append(&gtk4::SeparatorMenuItem::new());

        // Layout submenu
        let layout_item = gtk4::MenuItem::with_label("Layout");
        let layout_sub = gtk4::Menu::new();
        for l in &layouts {
            let mi = gtk4::MenuItem::with_label(&l.name);
            let proxy = proxy.clone();
            let id = l.id;
            mi.connect_activate(move |_| {
                let proxy = proxy.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    // Resolve primary monitor key on the fly
                    if let Ok(monitors) = proxy.list_monitors().await {
                        let key = monitors
                            .iter()
                            .find(|m| m.is_primary)
                            .map(|m| m.monitor_key.clone())
                            .or_else(|| monitors.first().map(|m| m.monitor_key.clone()))
                            .unwrap_or_default();
                        if !key.is_empty() {
                            let _ = proxy.assign_layout(&key, id).await;
                        }
                    }
                });
            });
            layout_sub.append(&mi);
        }
        layout_item.set_submenu(Some(&layout_sub));
        menu.append(&layout_item);

        menu.append(&gtk4::SeparatorMenuItem::new());

        let pause_item = gtk4::CheckMenuItem::with_label("Pause");
        pause_item.set_active(paused);
        {
            let proxy = proxy.clone();
            pause_item.connect_toggled(move |_| {
                let proxy = proxy.clone();
                gtk4::glib::MainContext::default().spawn_local(async move {
                    let _ = proxy.toggle_paused().await;
                });
            });
        }
        menu.append(&pause_item);

        let about = gtk4::MenuItem::with_label("About gnome-zones");
        menu.append(&about);

        menu.show_all();
        ind.set_menu(&mut menu.clone());

        Rc::new(Self { inner: RefCell::new(ind), menu })
    }

    /// Set the indicator icon to paused/unpaused.
    pub fn set_paused(&self, paused: bool) {
        let mut ind = self.inner.borrow_mut();
        ind.set_icon_full(
            if paused { "view-grid-symbolic" } else { "view-grid-symbolic" },
            "gnome-zones",
        );
        // (Future: ship a strikethrough variant icon in dist/icons/)
        let _ = paused;
    }

    /// Hand off the Edit-zones… menu item's activation to the caller.
    pub fn connect_edit_clicked<F: Fn() + 'static>(&self, cb: F) {
        // Find the "Edit zones…" menu item (second child).
        let children = self.menu.children();
        if let Some(item) = children.get(1).and_then(|c| c.downcast_ref::<gtk4::MenuItem>()) {
            item.connect_activate(move |_| cb());
        }
    }
}
```

Note: `libayatana-appindicator` builds on top of GTK3 menus, not GTK4. If the 0.9 crate still exposes a GTK3 menu API, use that — the imports above may need to be `gtk3::Menu` instead of `gtk4::Menu`. Verify the crate's current API and adjust.

- [ ] **Step 2: Create `src/panel/mod.rs`**

```rust
pub mod indicator;

pub use indicator::Indicator;
```

- [ ] **Step 3: Wire module into `main.rs`**

```rust
mod activator;
mod app;
mod dbus;
mod editor;
mod error;
mod overlay;
mod panel;
```

- [ ] **Step 4: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success (address any GTK3/GTK4 mismatch errors by matching the libayatana-appindicator crate's menu type).

- [ ] **Step 5: Commit**

```bash
git add crates/gnome-zones/src/
git commit -m "feat(zones-ui): panel AppIndicator + right-click menu"
```

---

## Task 18: Main — signal dispatcher and CLI routing

**Files:**
- Modify: `crates/gnome-zones/src/main.rs`

Glue everything together. In panel-icon mode (no CLI flags), the main loop:
1. Builds the panel indicator.
2. Subscribes to `ActivatorRequested`, `EditorRequested`, and `PausedChanged` signals.
3. On each signal, spawns the corresponding overlay.

In `--editor` / `--activator` mode, the process opens one overlay and exits when it closes.

- [ ] **Step 1: Replace the `connect_activate` body in `main.rs`**

```rust
mod activator;
mod app;
mod dbus;
mod editor;
mod error;
mod overlay;
mod panel;

use clap::Parser;
use futures_util::StreamExt;
use gtk4::prelude::*;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = app::Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let _guard = rt.enter();

    let proxy = rt
        .block_on(dbus::connect())
        .expect("failed to connect to org.gnome.Zones — is gnome-zones-daemon running?");

    let application = app::build_app();

    application.connect_activate(move |app| {
        if cli.editor {
            let app = app.clone();
            let proxy = proxy.clone();
            let mk = cli.monitor.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                let monitor_key = resolve_monitor_key(&proxy, mk).await;
                editor::show(&app, proxy, monitor_key);
            });
            return;
        }
        if cli.activator {
            let app = app.clone();
            let proxy = proxy.clone();
            let mk = cli.monitor.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                let monitor_key = resolve_monitor_key(&proxy, mk).await;
                let paused = is_paused(&proxy).await;
                activator::show(&app, proxy, monitor_key, paused);
            });
            return;
        }

        // Panel-icon mode: background process
        run_panel_mode(app, proxy.clone());
    });

    application.run();
    drop(rt);
}

async fn resolve_monitor_key(proxy: &dbus::ZonesProxy<'static>, preferred: Option<String>) -> String {
    if let Some(k) = preferred { return k; }
    let monitors = proxy.list_monitors().await.unwrap_or_default();
    monitors
        .iter()
        .find(|m| m.is_primary)
        .map(|m| m.monitor_key.clone())
        .or_else(|| monitors.first().map(|m| m.monitor_key.clone()))
        .unwrap_or_default()
}

async fn is_paused(proxy: &dbus::ZonesProxy<'static>) -> bool {
    let s = proxy.get_settings().await.unwrap_or_default();
    matches!(s.get("paused").map(String::as_str), Some("1") | Some("true"))
}

fn run_panel_mode(app: &gtk4::Application, proxy: dbus::ZonesProxy<'static>) {
    // Hold the application alive with a dummy hidden window so GtkApplication
    // doesn't quit after connect_activate returns.
    let hold = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(1).default_height(1)
        .visible(false)
        .build();
    app.add_window(&hold);
    hold.set_hide_on_close(true);
    hold.set_visible(false);

    let proxy_for_signals = proxy.clone();
    let app_weak = app.downgrade();

    gtk4::glib::MainContext::default().spawn_local(async move {
        // Initial fetch for panel menu
        let layouts = proxy_for_signals.list_layouts().await.unwrap_or_default();
        let paused  = is_paused(&proxy_for_signals).await;
        let Some(app) = app_weak.upgrade() else { return; };

        let indicator = panel::Indicator::new(proxy_for_signals.clone(), layouts, paused);

        // Edit zones… menu item → spawn editor on primary monitor
        let proxy_edit = proxy_for_signals.clone();
        let app_edit = app.clone();
        indicator.connect_edit_clicked(move || {
            let proxy = proxy_edit.clone();
            let app = app_edit.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                let mk = resolve_monitor_key(&proxy, None).await;
                editor::show(&app, proxy, mk);
            });
        });

        // Activator signal stream
        let proxy_act = proxy_for_signals.clone();
        let app_act = app.clone();
        gtk4::glib::MainContext::default().spawn_local(async move {
            if let Ok(mut stream) = proxy_act.receive_activator_requested().await {
                while let Some(sig) = stream.next().await {
                    if let Ok(args) = sig.args() {
                        let mk = args.monitor_key.clone();
                        let proxy = proxy_act.clone();
                        let app = app_act.clone();
                        gtk4::glib::MainContext::default().spawn_local(async move {
                            let paused = is_paused(&proxy).await;
                            activator::show(&app, proxy, mk, paused);
                        });
                    }
                }
            }
        });

        // Editor signal stream
        let proxy_ed = proxy_for_signals.clone();
        let app_ed = app.clone();
        gtk4::glib::MainContext::default().spawn_local(async move {
            if let Ok(mut stream) = proxy_ed.receive_editor_requested().await {
                while let Some(sig) = stream.next().await {
                    if let Ok(args) = sig.args() {
                        let mk = args.monitor_key.clone();
                        let proxy = proxy_ed.clone();
                        let app = app_ed.clone();
                        gtk4::glib::MainContext::default().spawn_local(async move {
                            editor::show(&app, proxy, mk);
                        });
                    }
                }
            }
        });

        // Paused signal → update indicator
        let indicator_paused = indicator.clone();
        let proxy_paused = proxy_for_signals.clone();
        gtk4::glib::MainContext::default().spawn_local(async move {
            if let Ok(mut stream) = proxy_paused.receive_paused_changed().await {
                while let Some(sig) = stream.next().await {
                    if let Ok(paused) = sig.args().map(|a| a.paused) {
                        indicator_paused.set_paused(paused);
                    }
                }
            }
        });
    });
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo build -p gnome-zones
```

Expected: success.

- [ ] **Step 3: Manual smoke test — full flow**

```bash
systemctl --user start gnome-zones-daemon
./target/debug/gnome-zones &
```

Expected: panel icon appears in the system tray. Trigger the daemon's `Super+Backquote` hotkey (or `gdbus call --session --dest org.gnome.Zones --object-path /org/gnome/Zones --method org.gnome.Zones.ShowActivator`) — activator overlay appears. Press `2` — window snaps to zone 2. Trigger `Super+Shift+E` (or emit `EditorRequested` manually) — editor overlay appears. Edit zones, Apply — closes and persists. Right-click the panel icon — menu shows layouts, Pause, Edit zones…, About.

- [ ] **Step 4: Commit**

```bash
git add crates/gnome-zones/src/main.rs
git commit -m "feat(zones-ui): main — signal dispatcher, panel mode, CLI routing"
```

---

## Task 19: Manual smoke-test checklist

**Files:**
- Create: `dist/test/ui-manual.md`

Document the verification steps that automation can't cover (visual overlay alpha, multi-monitor, panel icon across screen-lock).

- [ ] **Step 1: Create `dist/test/ui-manual.md`**

```markdown
# gnome-zones UI manual smoke test

Prerequisite: daemon running (`systemctl --user status gnome-zones-daemon`).
Start the UI: `./target/debug/gnome-zones &`

## Activator
- [ ] `Super+Backquote` → overlay appears over focused monitor within 200ms
- [ ] Numbers visible on each zone; semi-transparent blue fills
- [ ] Overlay does NOT steal focus from the previously-focused window
- [ ] Pressing `2` snaps the focused window to zone 2 and closes the overlay
- [ ] Pressing `Shift+3` snaps to zone 3 AND leaves the overlay open
- [ ] Pressing `Escape` closes the overlay without snapping
- [ ] After 3 seconds of inactivity the overlay auto-dismisses
- [ ] With `paused=true`, overlay shows "Paused" banner and digits are ignored

## Editor
- [ ] `Super+Shift+E` (or panel menu → Edit zones…) opens the editor on the focused monitor
- [ ] Translucent dark backdrop (~85% opacity) over desktop
- [ ] Zones render as blue rectangles with big centered numbers
- [ ] Click a zone → orange outline appears
- [ ] `+ Split horizontal` splits the selected zone top/bottom; numbers renumber row-major
- [ ] `+ Split vertical` splits the selected zone left/right
- [ ] `🗑 Delete` removes selected zone and grows an adjacent neighbor if one spans the edge
- [ ] Dragging a divider between two zones resizes both zones smoothly
- [ ] Click-drag in empty space draws a new zone; released zone appears with a new number
- [ ] Layout dropdown switches loaded layout without saving current edits
- [ ] `Save as…` creates a new user layout with the entered name
- [ ] `Apply` persists edits and closes; zones take effect on the next `Super+Ctrl+N`
- [ ] `Cancel` / `Esc` closes without saving
- [ ] Editing a preset and hitting Apply auto-forks (preset itself never changes)

## Panel icon
- [ ] Appindicator icon visible in the system tray on launch
- [ ] Right-click menu lists all layouts; selecting one assigns it to the primary monitor
- [ ] "Pause" toggle pauses/resumes snap hotkeys (confirmed via `Super+Ctrl+1`)
- [ ] "Edit zones…" opens the editor on the primary monitor
- [ ] Icon survives screen-lock (still present on unlock)

## Multi-monitor
- [ ] Trigger `Super+Backquote` on the secondary monitor → overlay covers *that* monitor
- [ ] Editor invoked from secondary monitor edits that monitor's layout, not the primary's
- [ ] Hot-plug: unplug secondary display, re-plug → previous layout assignment restored

## Stress
- [ ] Repeatedly fire `Super+Backquote` 10 times in 2 seconds — no crash, no zombie overlays
- [ ] Editor with 9 zones performs a divider drag without visible lag
```

- [ ] **Step 2: Commit**

```bash
git add dist/test/ui-manual.md
git commit -m "docs(zones): UI manual smoke-test checklist"
```

---

## Self-review summary

**Spec coverage:**
- §4 Hotkey scheme — handled by daemon; UI only subscribes to its signals ✓
- §5 Zone editor — Tasks 7-16 (state, render, drag, toolbar, save) ✓
- §6 Activator — Tasks 5-6 (state, view, keyboard, auto-dismiss, paused banner) ✓
- §7 Panel + pause + hot-plug — Task 17 (panel menu, pause toggle); hot-plug handled by daemon which emits `MonitorsChanged` (UI doesn't need to react for v1 since editor is spawned per-request) ✓
- §8 Wayland + X11 — Task 4 layer-shell helper covers both paths ✓
- §10 Testing — unit tests for every `state.rs` operation; manual checklist in Task 19 ✓

**Deferred (out of scope per §11):** drag-to-snap, per-app defaults, in-app settings window, pre-snap memory UI, workspace-aware layouts, import/export, multi-monitor editor.

**Critical implementation note from spec:** Activator must not steal focus from the originally-focused window. Task 4 + 6 address this via `KeyboardMode::OnDemand`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-14-gnome-zones-ui.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
