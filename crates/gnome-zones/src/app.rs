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
