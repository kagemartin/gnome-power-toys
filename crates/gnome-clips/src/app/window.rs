use gtk4::prelude::*;
use gtk4::ApplicationWindow;
use libadwaita as adw;
use libadwaita::prelude::*;

pub struct ClipsWindow {
    pub window: ApplicationWindow,
}

impl ClipsWindow {
    pub fn new(app: &adw::Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Clipboard History")
            .default_width(780)
            .default_height(460)
            .decorated(true)
            .build();

        let placeholder = gtk4::Label::new(Some("gnome-clips loading…"));
        window.set_child(Some(&placeholder));

        Self { window }
    }

    pub fn show(&self) {
        self.window.present();
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }
}
