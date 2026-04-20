//! Register (and reconcile) the GNOME keyboard shortcut that launches
//! `gnome-clips`. Uses `gio::Settings` directly — we do NOT shell out to
//! the `gsettings` CLI.
//!
//! Strategy (per the GNOME keybindings story):
//!
//! 1. Strip the target accelerator (e.g. `<Super>v`) from any pre-existing
//!    binding in `org.gnome.desktop.wm.keybindings`,
//!    `org.gnome.shell.keybindings`, and `org.gnome.mutter.keybindings`
//!    so it is free to claim.
//! 2. Upsert a relocatable custom-keybinding under
//!    `org.gnome.settings-daemon.plugins.media-keys.custom-keybinding` at
//!    our well-known path, and append that path to
//!    `org.gnome.settings-daemon.plugins.media-keys custom-keybindings`
//!    if not already present.
//! 3. `Settings::sync()` to flush writes to dconf before the settings
//!    daemon picks them up.

use gtk4::gio::{self, prelude::*, SettingsSchemaSource};

const CUSTOM_ID: &str = "gnome-clips";
const CUSTOM_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnome-clips/";
const CUSTOM_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CONFLICT_SCHEMAS: &[&str] = &[
    "org.gnome.desktop.wm.keybindings",
    "org.gnome.shell.keybindings",
    "org.gnome.mutter.keybindings",
];

pub fn register_shortcut(accelerator: &str) {
    if schema_source().is_none() {
        tracing::warn!("no GSettings schema source available — skipping shortcut registration");
        return;
    }

    clear_conflicts(accelerator);
    install_custom_binding(accelerator);

    gio::Settings::sync();
    tracing::info!(binding = accelerator, "keyboard shortcut registered");
}

/// Strip `accelerator` from any keybinding entry in the well-known GNOME
/// schemas so it no longer collides with the custom binding we install.
fn clear_conflicts(accelerator: &str) {
    let Some(source) = schema_source() else {
        return;
    };

    for schema_id in CONFLICT_SCHEMAS {
        let Some(schema) = source.lookup(schema_id, true) else {
            continue;
        };
        let settings = gio::Settings::new(schema_id);

        for key in schema.list_keys() {
            let key_str = key.as_str();
            // Only `as` (string-array) keys hold accelerator lists.
            let schema_key = schema.key(key_str);
            if schema_key.value_type().to_string() != "as" {
                continue;
            }

            let current: Vec<String> = settings
                .strv(key_str)
                .iter()
                .map(|s| s.to_string())
                .collect();
            let stripped = strip_conflict(&current, accelerator);
            if stripped != current {
                let refs: Vec<&str> = stripped.iter().map(String::as_str).collect();
                if let Err(e) = settings.set_strv(key_str, refs.as_slice()) {
                    tracing::warn!(schema = schema_id, key = key_str, %e, "failed to clear binding");
                } else {
                    tracing::info!(
                        schema = schema_id,
                        key = key_str,
                        "cleared conflicting accelerator"
                    );
                }
            }
        }
    }
}

fn install_custom_binding(accelerator: &str) {
    let media_keys = gio::Settings::new(MEDIA_KEYS_SCHEMA);

    let mut bindings: Vec<String> = media_keys
        .strv("custom-keybindings")
        .iter()
        .map(|s| s.to_string())
        .collect();
    if !bindings.iter().any(|p| p == CUSTOM_PATH) {
        bindings.push(CUSTOM_PATH.to_string());
        let refs: Vec<&str> = bindings.iter().map(String::as_str).collect();
        if let Err(e) = media_keys.set_strv("custom-keybindings", refs.as_slice()) {
            tracing::warn!(%e, "failed to extend custom-keybindings");
        }
    }

    let custom = gio::Settings::with_path(CUSTOM_SCHEMA, CUSTOM_PATH);
    set_string_logged(&custom, "name", "Clipboard History");
    set_string_logged(&custom, "command", CUSTOM_ID);
    set_string_logged(&custom, "binding", accelerator);
}

fn set_string_logged(settings: &gio::Settings, key: &str, value: &str) {
    if let Err(e) = settings.set_string(key, value) {
        tracing::warn!(key, %e, "failed to write setting");
    }
}

fn schema_source() -> Option<SettingsSchemaSource> {
    SettingsSchemaSource::default()
}

/// Pure: return `current` with any entry matching `accelerator` removed.
fn strip_conflict(current: &[String], accelerator: &str) -> Vec<String> {
    current
        .iter()
        .filter(|entry| !accel_matches(entry, accelerator))
        .cloned()
        .collect()
}

/// Accelerator-string equality. Case-insensitive so `<Super>v` matches
/// `<super>V`. Whitespace is stripped so `"<Super> v"` matches `"<Super>v"`.
fn accel_matches(a: &str, b: &str) -> bool {
    normalize_accel(a) == normalize_accel(b)
}

fn normalize_accel(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_matches_is_case_insensitive() {
        assert!(accel_matches("<Super>v", "<super>V"));
        assert!(accel_matches("<Primary><Super>v", "<primary><super>V"));
    }

    #[test]
    fn accel_matches_ignores_whitespace() {
        assert!(accel_matches("<Super> v", "<Super>v"));
    }

    #[test]
    fn accel_matches_differentiates_distinct_accelerators() {
        assert!(!accel_matches("<Super>v", "<Super>c"));
        assert!(!accel_matches("<Super>v", "<Control>v"));
    }

    #[test]
    fn strip_conflict_removes_target_and_preserves_others() {
        let current = vec![
            "<Super>v".to_string(),
            "<Control><Alt>t".to_string(),
            "<Primary>c".to_string(),
        ];
        let stripped = strip_conflict(&current, "<Super>v");
        assert_eq!(
            stripped,
            vec!["<Control><Alt>t".to_string(), "<Primary>c".to_string()]
        );
    }

    #[test]
    fn strip_conflict_removes_case_insensitive_matches() {
        let current = vec!["<super>V".to_string(), "<Control>a".to_string()];
        let stripped = strip_conflict(&current, "<Super>v");
        assert_eq!(stripped, vec!["<Control>a".to_string()]);
    }

    #[test]
    fn strip_conflict_returns_same_list_when_no_match() {
        let current = vec!["<Super>Left".to_string(), "<Super>Right".to_string()];
        let stripped = strip_conflict(&current, "<Super>v");
        assert_eq!(stripped, current);
    }

    #[test]
    fn strip_conflict_handles_empty_input() {
        assert!(strip_conflict(&[], "<Super>v").is_empty());
    }

    #[test]
    fn strip_conflict_drops_all_copies_of_duplicates() {
        let current = vec!["<Super>v".to_string(), "<Super>v".to_string()];
        assert!(strip_conflict(&current, "<Super>v").is_empty());
    }
}
