//! Panel tray indicator backed by the `ksni` crate (pure-Rust
//! StatusNotifierItem). Replaces `libayatana-appindicator` 0.9 which requires
//! GTK3; gnome-zones' UI is GTK4, so a GTK-agnostic tray implementation is
//! required.
//!
//! The tray service runs on the tokio runtime set up in `main.rs`. Menu
//! activations fire on tokio worker threads, so we forward them to the GTK
//! main context via an [`async_channel`] `Receiver<TrayEvent>` that Group G
//! drains with `glib::MainContext::spawn_local`.

use async_channel::{Receiver, Sender};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{Handle, MenuItem, Tray, TrayMethods};
use tokio::runtime::Handle as RtHandle;
use tracing::{debug, info, warn};

use crate::dbus::LayoutSummaryWire;

/// Events produced by tray menu clicks. Consumed by Group G's main
/// dispatcher on the GTK main context.
#[derive(Debug, Clone)]
pub enum TrayEvent {
    /// Left-click on the tray icon, or "Show activator" menu item.
    ShowActivator,
    /// "Edit zones…" menu item.
    ShowEditor,
    /// Submenu entry: a specific layout was chosen (assign to primary monitor).
    AssignLayout(i64),
    /// "Pause" checkmark toggle — daemon's paused flag is source of truth;
    /// tray only requests a toggle.
    TogglePaused,
}

/// Internal tray state. `ksni` requires `Send + 'static`.
struct PanelTray {
    layouts: Vec<LayoutSummaryWire>,
    paused: bool,
    event_tx: Sender<TrayEvent>,
}

impl PanelTray {
    fn dispatch(tx: &Sender<TrayEvent>, event: TrayEvent) {
        // try_send never blocks. If the channel is closed or full the event
        // is dropped — that's acceptable here (tray clicks are idempotent
        // user gestures, not state mutations).
        match tx.try_send(event.clone()) {
            Ok(()) => debug!(?event, "tray event queued"),
            Err(e) => warn!(?event, error = %e, "failed to forward tray event"),
        }
    }
}

impl Tray for PanelTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "gnome-zones".into()
    }

    fn icon_name(&self) -> String {
        "view-grid-symbolic".into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: "view-grid-symbolic".into(),
            icon_pixmap: Vec::new(),
            title: "gnome-zones".into(),
            description: if self.paused {
                "Paused".into()
            } else {
                "Active".into()
            },
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        // Left-click on the icon surfaces the activator.
        Self::dispatch(&self.event_tx, TrayEvent::ShowActivator);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        // Layout submenu (one entry per layout, assigns to primary monitor).
        let layout_items: Vec<MenuItem<Self>> = self
            .layouts
            .iter()
            .map(|summary| {
                let label = summary.name.clone();
                let id = summary.id;
                StandardItem {
                    label,
                    activate: Box::new(move |this: &mut Self| {
                        Self::dispatch(&this.event_tx, TrayEvent::AssignLayout(id));
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();

        let layout_submenu: MenuItem<Self> = if layout_items.is_empty() {
            SubMenu {
                label: "Layout".into(),
                enabled: false,
                submenu: vec![StandardItem {
                    label: "(no layouts)".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into()
        } else {
            SubMenu {
                label: "Layout".into(),
                submenu: layout_items,
                ..Default::default()
            }
            .into()
        };

        vec![
            StandardItem {
                label: "Show activator".into(),
                activate: Box::new(|this: &mut Self| {
                    Self::dispatch(&this.event_tx, TrayEvent::ShowActivator);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Edit zones…".into(),
                activate: Box::new(|this: &mut Self| {
                    Self::dispatch(&this.event_tx, TrayEvent::ShowEditor);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            layout_submenu,
            MenuItem::Separator,
            CheckmarkItem {
                label: "Pause".into(),
                checked: self.paused,
                activate: Box::new(|this: &mut Self| {
                    Self::dispatch(&this.event_tx, TrayEvent::TogglePaused);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "About gnome-zones".into(),
                activate: Box::new(|_this: &mut Self| {
                    info!("About gnome-zones clicked");
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Public opaque handle that keeps the ksni service alive and lets Group G
/// update tray state as D-Bus signals arrive.
pub struct Indicator {
    handle: Handle<PanelTray>,
    rt: RtHandle,
}

impl Indicator {
    /// Spawn the tray on the given tokio runtime handle. Returns the handle
    /// plus an [`async_channel::Receiver`] that Group G drains on the GTK
    /// main context.
    ///
    /// The caller must provide a [`tokio::runtime::Handle`] because the
    /// underlying `ksni::TrayMethods::spawn` is async and the tray runs on
    /// tokio workers. Using an explicit handle lets this function be called
    /// from a non-tokio thread (e.g. before the GTK main loop starts).
    pub fn spawn(
        rt: RtHandle,
        layouts: Vec<LayoutSummaryWire>,
        paused: bool,
    ) -> Result<(Self, Receiver<TrayEvent>), ksni::Error> {
        // Unbounded channel: tray clicks are sparse and we must never block
        // the tokio worker running the menu closure.
        let (event_tx, event_rx) = async_channel::unbounded::<TrayEvent>();

        let tray = PanelTray {
            layouts,
            paused,
            event_tx,
        };

        // Drive the async `spawn` on the provided runtime.
        let handle = rt.block_on(async move { tray.spawn().await })?;

        Ok((Self { handle, rt }, event_rx))
    }

    /// Update the paused flag (called when `PausedChanged` signal arrives).
    pub fn set_paused(&self, paused: bool) {
        let handle = self.handle.clone();
        // `Handle::update` is async; fire-and-forget on the tokio runtime.
        self.rt.spawn(async move {
            handle
                .update(move |tray: &mut PanelTray| {
                    tray.paused = paused;
                })
                .await;
        });
    }

    /// Replace the layout list (called when `LayoutsChanged` signal arrives).
    pub fn set_layouts(&self, layouts: Vec<LayoutSummaryWire>) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            handle
                .update(move |tray: &mut PanelTray| {
                    tray.layouts = layouts;
                })
                .await;
        });
    }

    /// Shut the tray service down. Idempotent; safe to call from Drop path.
    pub fn shutdown(&self) {
        let _ = self.handle.shutdown();
    }
}

impl Drop for Indicator {
    fn drop(&mut self) {
        // `shutdown()` returns a ShutdownAwaiter we deliberately discard.
        // The ksni service loop runs on the tokio runtime; it will actually
        // drain and unregister from D-Bus when `main.rs` does `drop(rt)`
        // after `application.run()` returns. No synchronous wait needed here.
        let _ = self.handle.shutdown();
    }
}
