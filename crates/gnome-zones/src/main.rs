mod activator;
mod app;
mod dbus;
mod editor;
mod error;
mod overlay;
mod panel;

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
        let _ = (app, &proxy, &rt_handle);
        tracing::info!(editor = cli.editor, activator = cli.activator, "gnome-zones launched");
    });

    application.run();
    drop(rt);
}
