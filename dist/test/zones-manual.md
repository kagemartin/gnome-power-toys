# gnome-zones Manual Smoke Test

Run on a real GNOME session. These test paths no unit test can cover.

## Prereqs

1. Install `gnome-zones-mover@power-toys`:
   ```bash
   mkdir -p ~/.local/share/gnome-shell/extensions/
   cp -r dist/shell-extension/gnome-zones-mover@power-toys/ \
         ~/.local/share/gnome-shell/extensions/
   ```
   Log out and back in (X11) or press `Alt+F2` `r` `Enter` (X11 only — on Wayland you must log out).
   Then enable:
   ```bash
   gnome-extensions enable gnome-zones-mover@power-toys
   ```

2. Build and run the daemon:
   ```bash
   cargo run -p gnome-zones-daemon
   ```

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
