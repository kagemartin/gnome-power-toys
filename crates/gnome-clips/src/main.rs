mod app;
mod dbus;
mod error;

use gtk4::glib;
use libadwaita::prelude::*;

use app::window::ClipsWindow;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let proxy = glib::MainContext::default()
        .block_on(dbus::connect())
        .expect("failed to connect to gnome-clips-daemon — is the daemon running?");

    let application = app::build_app();

    application.connect_activate(move |app| {
        let window = ClipsWindow::new(app, proxy.clone());
        window.show();
    });

    application.run();
}
