mod app;
mod dbus;
mod error;

use libadwaita::prelude::*;

use app::window::ClipsWindow;

fn main() {
    let application = app::build_app();

    application.connect_activate(|app| {
        let window = ClipsWindow::new(app);
        window.show();
    });

    application.run();
}
