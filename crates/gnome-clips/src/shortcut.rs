//! Legacy cleanup. Earlier builds registered `<Super>v` as a GNOME
//! "Custom Shortcut" under the media-keys plugin. That mechanism launches
//! a command when the key fires, which isn't the right abstraction for a
//! session-long GtkApplication — and on Wayland, it doesn't fire at all
//! once the shell is holding the grab.
//!
//! The keybinding now lives in the `gnome-clips-toggle@power-toys` shell
//! extension (see `dist/shell-extension/`), which uses
//! `Main.wm.addKeybinding` and talks to us via D-Bus.
//!
//! This module only removes any stale custom-keybinding entry we wrote in
//! older builds so users don't end up with a dead entry in GNOME Settings.
//! It's idempotent and safe to call on every startup.
//!
//! Delete this module once nobody is upgrading from a pre-extension build.

use gtk4::gio::{self, prelude::*, SettingsSchemaSource};

const CUSTOM_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnome-clips/";
const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";

pub fn cleanup_legacy_media_keys_entry() {
    let Some(source) = SettingsSchemaSource::default() else {
        return;
    };
    if source.lookup(MEDIA_KEYS_SCHEMA, true).is_none() {
        return;
    }

    let media_keys = gio::Settings::new(MEDIA_KEYS_SCHEMA);
    let current: Vec<String> = media_keys
        .strv("custom-keybindings")
        .iter()
        .map(|s| s.to_string())
        .collect();
    let pruned = remove_path(&current, CUSTOM_PATH);
    if pruned != current {
        let refs: Vec<&str> = pruned.iter().map(String::as_str).collect();
        if let Err(e) = media_keys.set_strv("custom-keybindings", refs.as_slice()) {
            tracing::warn!(%e, "failed to prune legacy custom-keybindings list");
            return;
        }
        // Blank out the relocatable binding so GNOME Settings won't show a
        // ghost row with the old accelerator.
        let custom = gio::Settings::with_path(CUSTOM_SCHEMA, CUSTOM_PATH);
        let _ = custom.set_string("name", "");
        let _ = custom.set_string("command", "");
        let _ = custom.set_string("binding", "");
        gio::Settings::sync();
        tracing::info!("removed legacy media-keys custom-binding for gnome-clips");
    }
}

fn remove_path(list: &[String], target: &str) -> Vec<String> {
    list.iter().filter(|p| *p != target).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_path_drops_single_match() {
        let list = vec![
            "/a/".to_string(),
            CUSTOM_PATH.to_string(),
            "/b/".to_string(),
        ];
        assert_eq!(
            remove_path(&list, CUSTOM_PATH),
            vec!["/a/".to_string(), "/b/".to_string()]
        );
    }

    #[test]
    fn remove_path_noop_when_absent() {
        let list = vec!["/a/".to_string(), "/b/".to_string()];
        assert_eq!(remove_path(&list, CUSTOM_PATH), list);
    }

    #[test]
    fn remove_path_handles_empty() {
        assert!(remove_path(&[], CUSTOM_PATH).is_empty());
    }
}
