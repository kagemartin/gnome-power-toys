pub mod clip_list;
pub mod clip_row;
pub mod filter_bar;
pub mod preview_pane;
pub mod window;

use libadwaita as adw;
use libadwaita::prelude::*;

// DISTINCT from the daemon's bus name "org.gnome.Clips" — GtkApplication owns
// this name on the session bus, so it must not collide with the daemon.
pub const APP_ID: &str = "org.gnome.Clips.Ui";

pub fn build_app() -> adw::Application {
    adw::Application::builder()
        .application_id(APP_ID)
        .build()
}
