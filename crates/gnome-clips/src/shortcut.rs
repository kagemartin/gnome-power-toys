use std::process::Command;

/// Registers a GNOME custom keyboard shortcut that launches `gnome-clips`.
/// Safe to call on every launch — `gsettings` upserts the shortcut list.
pub fn register_shortcut(binding: &str) {
    let base = "org.gnome.settings-daemon.plugins.media-keys";
    let key = "custom-keybindings";
    let path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnome-clips/";

    let existing = Command::new("gsettings")
        .args(["get", base, key])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "@as []".to_string());

    let already_registered = existing.contains("gnome-clips");

    if !already_registered {
        let trimmed = existing.trim();
        let new_list = if trimmed == "@as []" || trimmed == "[]" {
            format!("['{}']", path)
        } else {
            let without_close = trimmed.trim_end_matches(']');
            format!("{}, '{}']", without_close, path)
        };

        let _ = Command::new("gsettings")
            .args(["set", base, key, &new_list])
            .status();
    }

    let custom_base = format!(
        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{}",
        path
    );
    let _ = Command::new("gsettings")
        .args(["set", &custom_base, "name", "Clipboard History"])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", &custom_base, "command", "gnome-clips"])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", &custom_base, "binding", binding])
        .status();

    tracing::info!(%binding, "keyboard shortcut registered");
}
