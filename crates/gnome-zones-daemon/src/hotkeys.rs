// crates/gnome-zones-daemon/src/hotkeys.rs
//
// Hotkeys themselves are registered by the `gnome-zones-mover` shell extension
// via `Main.wm.addKeybinding`. This module only manages *conflict resolution*:
// stash the stock GNOME bindings that overlap our accelerators and set them to
// `[]`, so the extension's grabs succeed. `restore_gnome_defaults` undoes this.
use crate::db::{settings, Database};
use crate::error::{Error, Result};
use std::process::Command;

const MUTTER_KB_SCHEMA: &str = "org.gnome.mutter.keybindings";
const SHELL_KB_SCHEMA: &str = "org.gnome.shell.keybindings";

/// (schema, gsettings-key, our-stash-key)
fn conflict_entries() -> Vec<(&'static str, String, String)> {
    let mut v = Vec::new();
    // Super+Left / Super+Right — we use these for iter-prev/next.
    v.push((
        MUTTER_KB_SCHEMA,
        "toggle-tiled-left".to_string(),
        "gnome_default_tile_left".to_string(),
    ));
    v.push((
        MUTTER_KB_SCHEMA,
        "toggle-tiled-right".to_string(),
        "gnome_default_tile_right".to_string(),
    ));
    // Super+Ctrl+1..9 — we use these for snap-1..9.
    for n in 1..=9 {
        v.push((
            SHELL_KB_SCHEMA,
            format!("open-new-window-application-{n}"),
            format!("gnome_default_open_new_window_application_{n}"),
        ));
    }
    v
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

/// Stash GNOME's default bindings that conflict with our accelerators and set
/// them to `[]`. Idempotent — a previously-stashed entry is skipped.
pub fn stash_gnome_defaults(db: &Database) -> Result<()> {
    for (schema, gkey, our_key) in conflict_entries() {
        if settings::get_setting(db, &our_key)?.is_some() {
            continue;
        }
        let current = gsettings_get(schema, &gkey)?;
        settings::set_setting(db, &our_key, &current)?;
        gsettings_set(schema, &gkey, "[]")?;
    }
    Ok(())
}

/// Restore the stashed GNOME defaults. Called on uninstall or by the user.
#[allow(dead_code)]
pub fn restore_gnome_defaults(db: &Database) -> Result<()> {
    for (schema, gkey, our_key) in conflict_entries() {
        if let Some(stashed) = settings::get_setting(db, &our_key)? {
            gsettings_set(schema, &gkey, &stashed)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_entries_covers_tiling_and_super_ctrl_n() {
        let e = conflict_entries();
        let keys: Vec<&str> = e.iter().map(|(_, k, _)| k.as_str()).collect();
        assert!(keys.contains(&"toggle-tiled-left"));
        assert!(keys.contains(&"toggle-tiled-right"));
        for n in 1..=9 {
            let k = format!("open-new-window-application-{n}");
            assert!(
                keys.iter().any(|x| *x == k),
                "missing entry: {k}"
            );
        }
    }

    #[test]
    fn conflict_entries_stash_keys_are_unique() {
        let e = conflict_entries();
        let mut stash: Vec<&str> = e.iter().map(|(_, _, s)| s.as_str()).collect();
        stash.sort();
        let len = stash.len();
        stash.dedup();
        assert_eq!(stash.len(), len, "duplicate stash keys");
    }
}
