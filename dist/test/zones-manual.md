# gnome-zones Manual Smoke Test

Run on a real GNOME session. These test paths no unit test can cover.

## Prereqs

1. Install `gnome-zones-mover@power-toys`:
   ```bash
   EXT=~/.local/share/gnome-shell/extensions/gnome-zones-mover@power-toys
   mkdir -p "$EXT"
   cp -r dist/shell-extension/gnome-zones-mover@power-toys/. "$EXT/"
   glib-compile-schemas "$EXT/schemas/"
   ```
   Log out and back in (X11) or press `Alt+F2` `r` `Enter` (X11 only — on Wayland you must log out).
   Then enable:
   ```bash
   gnome-extensions enable gnome-zones-mover@power-toys
   ```
   The extension owns both the `org.gnome.Shell.Extensions.GnomeZonesMover`
   window-mover API and every keybinding (registered via
   `Main.wm.addKeybinding`). If the extension isn't loaded, no hotkey fires.

2. Build and run the daemon:
   ```bash
   cargo run -p gnome-zones-daemon
   ```
   On first run it stashes the stock GNOME bindings that would conflict with
   ours (`org.gnome.mutter.keybindings.toggle-tiled-left/right` and
   `org.gnome.shell.keybindings.open-new-window-application-1..9`) and sets
   them to `[]` so the extension's grabs succeed. These are restorable from
   the DB if needed.

## Tests

### T1: D-Bus surface
```bash
busctl --user introspect org.gnome.Zones /org/gnome/Zones
```
Expect: `org.gnome.Zones` interface listed with every method from the spec.

### T2: Preset seeding
```bash
busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones ListLayouts
```
Expect: 8 preset layouts listed.

### T3: Snap to zone 1
Focus any window. Press `Super+Ctrl+1`. Expect: window resizes to cover the left half.

### T4: Snap to zone 2
Press `Super+Ctrl+2`. Expect: window resizes to cover the right half.

### T5: Iterate
Snap to zone 1, then press `Super+Right`. Expect: window moves to zone 2. Press again. Expect: wraps back to zone 1.

### T6: Activator
Press `` Super+` ``. Expect: `ActivatorRequested` signal is emitted (verify with `dbus-monitor`). No visual UI yet — that's Plan 2.

### T7: Span
Snap to zone 1, then press `Super+Alt+2`. Expect: window spans both halves (full width minus gap).

### T8: Pause
Press `Super+Shift+P`. Try `Super+Ctrl+1`. Expect: no movement (paused). Press `Super+Shift+P` again. Expect: shortcuts work again.

### T9: Restore defaults
Stop the daemon. Run:
```bash
busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones SetSetting ss "paused" "true"
```
(Set any value to verify the DB path works.) Kill the daemon. Check that `~/.local/share/gnome-zones/zones.db` exists.
