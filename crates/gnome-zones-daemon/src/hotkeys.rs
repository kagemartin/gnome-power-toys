// crates/gnome-zones-daemon/src/hotkeys.rs
use crate::db::{settings, Database};
use crate::error::{Error, Result};
use std::process::Command;

const KEYBIND_PREFIX: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnome-zones";
const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_BINDING_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const MUTTER_KB_SCHEMA: &str = "org.gnome.mutter.keybindings";

/// (schema-relative slug, display name, accelerator, busctl command)
///
/// The slugs become `/.../gnome-zones-<slug>/` object paths.
pub fn default_bindings() -> Vec<(&'static str, &'static str, &'static str, String)> {
    let busctl = |method: &str, args: &str| -> String {
        format!(
            "busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones {method} {args}"
        )
    };
    let mut out = Vec::new();
    for n in 1..=9 {
        out.push((
            Box::leak(format!("snap-{n}").into_boxed_str()) as &str,
            Box::leak(format!("Snap to zone {n}").into_boxed_str()) as &str,
            Box::leak(format!("<Super><Control>{n}").into_boxed_str()) as &str,
            busctl("SnapFocusedToZone", &format!("ub {n} false")),
        ));
        out.push((
            Box::leak(format!("span-{n}").into_boxed_str()) as &str,
            Box::leak(format!("Span into zone {n}").into_boxed_str()) as &str,
            Box::leak(format!("<Super><Alt>{n}").into_boxed_str()) as &str,
            busctl("SnapFocusedToZone", &format!("ub {n} true")),
        ));
    }
    out.push((
        "activator",
        "Show zone activator",
        "<Super>grave",
        busctl("ShowActivator", ""),
    ));
    out.push((
        "iter-prev",
        "Iterate to previous zone",
        "<Super>Left",
        busctl("IterateFocusedZone", "s prev"),
    ));
    out.push((
        "iter-next",
        "Iterate to next zone",
        "<Super>Right",
        busctl("IterateFocusedZone", "s next"),
    ));
    out.push((
        "cycle-prev",
        "Cycle focus back in zone",
        "<Super>Page_Up",
        busctl("CycleFocusInZone", "i -1"),
    ));
    out.push((
        "cycle-next",
        "Cycle focus forward in zone",
        "<Super>Page_Down",
        busctl("CycleFocusInZone", "i 1"),
    ));
    out.push((
        "editor",
        "Open zone editor",
        "<Super><Shift>e",
        busctl("ShowEditor", ""),
    )); // placeholder — Plan 2 adds ShowEditor
    out.push((
        "pause",
        "Toggle pause",
        "<Super><Shift>p",
        busctl("TogglePaused", ""),
    ));
    out
}

fn run(cmd: &mut Command) -> Result<String> {
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(Error::Config(format!(
            "gsettings exited {:?}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn gsettings_get(schema: &str, key: &str) -> Result<String> {
    run(Command::new("gsettings").args(["get", schema, key]))
}

fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<()> {
    run(Command::new("gsettings").args(["set", schema, key, value]))?;
    Ok(())
}

fn gsettings_set_with_path(schema: &str, path: &str, key: &str, value: &str) -> Result<()> {
    run(Command::new("gsettings")
        .args(["set", &format!("{schema}:{path}"), key, value]))?;
    Ok(())
}

/// Stash GNOME's default `Super+Left/Right` bindings and disable them.
/// Idempotent — calling again is a no-op once stashed.
pub fn stash_gnome_defaults(db: &Database) -> Result<()> {
    for (gkey, our_key) in [
        ("toggle-tiled-left", "gnome_default_tile_left"),
        ("toggle-tiled-right", "gnome_default_tile_right"),
    ] {
        if settings::get_setting(db, our_key)?.is_some() {
            continue; // already stashed on a previous run
        }
        let current = gsettings_get(MUTTER_KB_SCHEMA, gkey)?;
        settings::set_setting(db, our_key, &current)?;
        gsettings_set(MUTTER_KB_SCHEMA, gkey, "[]")?;
    }
    Ok(())
}

/// Restore the stashed GNOME defaults. Called on uninstall or by the user.
pub fn restore_gnome_defaults(db: &Database) -> Result<()> {
    for (gkey, our_key) in [
        ("toggle-tiled-left", "gnome_default_tile_left"),
        ("toggle-tiled-right", "gnome_default_tile_right"),
    ] {
        if let Some(stashed) = settings::get_setting(db, our_key)? {
            gsettings_set(MUTTER_KB_SCHEMA, gkey, &stashed)?;
        }
    }
    Ok(())
}

/// Register all of our custom keybindings via gsettings. Idempotent.
pub fn register_custom_bindings() -> Result<()> {
    let bindings = default_bindings();
    let mut paths: Vec<String> = Vec::with_capacity(bindings.len());
    for (slug, name, accel, command) in &bindings {
        let path = format!("{KEYBIND_PREFIX}-{slug}/");
        paths.push(path.clone());
        gsettings_set_with_path(
            CUSTOM_BINDING_SCHEMA,
            &path,
            "name",
            &format!("'{name}'"),
        )?;
        gsettings_set_with_path(
            CUSTOM_BINDING_SCHEMA,
            &path,
            "command",
            &format!("'{}'", command.replace('\'', "\\'")),
        )?;
        gsettings_set_with_path(
            CUSTOM_BINDING_SCHEMA,
            &path,
            "binding",
            &format!("'{accel}'"),
        )?;
    }
    // Register all paths in the media-keys custom-keybindings array.
    let array_value = format!(
        "[{}]",
        paths
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    gsettings_set(MEDIA_KEYS_SCHEMA, "custom-keybindings", &array_value)?;
    Ok(())
}

/// Remove our custom keybindings entirely. Called on uninstall.
pub fn unregister_custom_bindings() -> Result<()> {
    gsettings_set(MEDIA_KEYS_SCHEMA, "custom-keybindings", "[]")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_includes_nine_snap_plus_nine_span() {
        let b = default_bindings();
        assert_eq!(
            b.iter()
                .filter(|(slug, _, _, _)| slug.starts_with("snap-"))
                .count(),
            9
        );
        assert_eq!(
            b.iter()
                .filter(|(slug, _, _, _)| slug.starts_with("span-"))
                .count(),
            9
        );
    }

    #[test]
    fn default_bindings_has_all_navigation_entries() {
        let b = default_bindings();
        let slugs: Vec<&str> = b.iter().map(|(s, _, _, _)| *s).collect();
        for expected in &[
            "activator",
            "iter-prev",
            "iter-next",
            "cycle-prev",
            "cycle-next",
            "editor",
            "pause",
        ] {
            assert!(
                slugs.contains(expected),
                "missing binding: {expected}"
            );
        }
    }

    #[test]
    fn snap_binding_uses_super_ctrl_chord() {
        let b = default_bindings();
        let (_, _, accel, _) = b
            .iter()
            .find(|(s, _, _, _)| *s == "snap-1")
            .unwrap();
        assert_eq!(*accel, "<Super><Control>1");
    }

    #[test]
    fn span_binding_uses_super_alt_chord() {
        let b = default_bindings();
        let (_, _, accel, _) = b
            .iter()
            .find(|(s, _, _, _)| *s == "span-1")
            .unwrap();
        assert_eq!(*accel, "<Super><Alt>1");
    }
}
