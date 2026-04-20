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
    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_string(
        ".gnome-zones-overlay { background: rgba(0, 0, 0, 0); }\n\
         .gnome-zones-editor-backdrop { background: rgba(0, 0, 0, 0.85); }\n\
         .gnome-zones-zone { background: rgba(60, 120, 220, 0.25); \
           border: 2px solid rgba(120, 180, 255, 0.9); border-radius: 4px; }\n\
         .gnome-zones-zone-selected { border: 2px solid rgba(255, 160, 40, 1.0); }\n\
         .gnome-zones-zone-number { color: rgba(255, 255, 255, 0.9); \
           font-size: 96pt; font-weight: bold; }\n\
         .gnome-zones-divider { background: rgba(255, 255, 255, 0.4); border-radius: 3px; }\n",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    app
}
