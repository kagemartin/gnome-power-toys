use ksni::menu::StandardItem;
use ksni::{Handle, MenuItem, Tray, TrayService};

pub enum PanelEvent {
    Activate,
    ToggleIncognito,
    Quit,
}

pub struct ClipsTray {
    pub incognito: bool,
    pub tx: async_channel::Sender<PanelEvent>,
}

impl Tray for ClipsTray {
    fn id(&self) -> String {
        "gnome-clips".into()
    }

    fn title(&self) -> String {
        "Clipboard History".into()
    }

    fn icon_name(&self) -> String {
        if self.incognito {
            "changes-prevent-symbolic".into()
        } else {
            "edit-paste-symbolic".into()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(PanelEvent::Activate);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Open Clipboard History".into(),
                activate: Box::new(|t: &mut ClipsTray| {
                    let _ = t.tx.try_send(PanelEvent::Activate);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: if self.incognito {
                    "Exit Incognito".into()
                } else {
                    "Enter Incognito".into()
                },
                activate: Box::new(|t: &mut ClipsTray| {
                    let _ = t.tx.try_send(PanelEvent::ToggleIncognito);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut ClipsTray| {
                    let _ = t.tx.try_send(PanelEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the SNI service on its own thread. Callers drain the returned
/// receiver from the GLib main loop.
pub fn spawn() -> (Handle<ClipsTray>, async_channel::Receiver<PanelEvent>) {
    let (tx, rx) = async_channel::unbounded();
    let tray = ClipsTray {
        incognito: false,
        tx,
    };
    let service = TrayService::new(tray);
    let handle = service.handle();
    service.spawn();
    (handle, rx)
}
