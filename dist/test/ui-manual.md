# gnome-zones UI manual smoke test

Tests paths that automation can't cover — visual overlay alpha, multi-monitor
geometry, tray icon lifetime, Wayland vs. X11 rendering differences.

## Prerequisites

1. `gnome-zones-daemon` running and registered:
   ```bash
   systemctl --user start gnome-zones-daemon
   systemctl --user status gnome-zones-daemon
   ```

2. At least one layout assigned to the primary monitor (the 2-zone preset
   "Two Columns" is shipped by default — verify with
   `busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones ListLayouts`).

3. Build the UI:
   ```bash
   cargo build -p gnome-zones
   ./target/debug/gnome-zones &
   ```

## Activator

- [ ] `Super+Backquote` (or
      `busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones ShowActivator`)
      causes an overlay to appear over the focused monitor within ~200ms.
- [ ] Numbered zones visible; each zone is semi-transparent blue with a large
      centered digit.
- [ ] Overlay does NOT steal focus from the previously-focused window — you
      can still type into that window while the overlay is up.
- [ ] Pressing a digit `N` in `1..=zone_count` snaps the focused window to
      zone `N` and closes the overlay.
- [ ] Pressing `Shift+N` snaps to zone `N` AND leaves the overlay open (span
      mode).
- [ ] Pressing `Escape` closes the overlay without snapping.
- [ ] Pressing any non-digit / non-Escape key closes the overlay without
      snapping.
- [ ] After 3 seconds of inactivity the overlay auto-dismisses.
- [ ] With `paused=true`
      (`busctl --user call org.gnome.Zones /org/gnome/Zones org.gnome.Zones TogglePaused`),
      overlay shows "gnome-zones is paused" banner instead of zones and digits
      are ignored. Escape still dismisses.

## Editor

- [ ] `Super+Shift+E` (or tray menu → "Edit zones…") opens the editor on the
      focused monitor within ~200ms.
- [ ] Translucent dark backdrop (~85% opacity) covers the desktop.
- [ ] Zones render as translucent blue rectangles with large centered digits.
- [ ] Clicking a zone adds an orange border (selection indicator).
- [ ] "+ Split horizontal" splits the selected zone into top/bottom halves;
      numbers renumber row-major.
- [ ] "+ Split vertical" splits the selected zone into left/right halves.
- [ ] "Delete" removes the selected zone; if exactly one neighbor shares the
      deleted zone's full edge, that neighbor grows to absorb the deleted
      area.
- [ ] Dragging a divider between two zones resizes both smoothly — no jitter,
      no stuck state after the first tick, no runaway acceleration.
- [ ] Click-and-drag in empty space (outside any zone) draws a ghost
      rectangle; releasing creates a new zone at those bounds (only if width
      and height each exceed ~5% of the monitor).
- [ ] Layout dropdown switches to a different layout without prompting to
      save the current edits (warning: edits are discarded).
- [ ] "Reset" restores the layout to the state it was in when the editor
      opened.
- [ ] "+ New from current" clears the layout id and appends " (copy)" to the
      name. Subsequent Apply will create a new user layout.
- [ ] "Save as…" opens a dialog pre-filled with `"<current> (copy)"`. Saving
      creates a new user layout, updates the editor state to the new layout,
      and refreshes the layout dropdown so the new name appears.
- [ ] "Apply" persists edits (via UpdateLayout or CreateLayout if no id),
      assigns the resulting layout to the monitor, and closes the editor.
      Super+Ctrl+N now uses the new zones.
- [ ] Editing a preset and pressing Apply auto-forks: a new user layout is
      created (preset itself remains unchanged, verified via ListLayouts).
- [ ] If the daemon is stopped mid-Apply (assign_layout fails), the editor
      window stays open and an `error` line appears in `journalctl --user -t
      gnome-zones`.
- [ ] "Cancel" closes without saving.
- [ ] Changing the gap spinner fires `SetSetting gap_px N` — verify with
      `dbus-monitor --session "interface='org.gnome.Zones'"`.

## Panel tray icon

- [ ] Tray icon (view-grid-symbolic) appears in the system tray within ~1s
      of launching `gnome-zones`.
- [ ] Left-click on the icon invokes `ShowActivator` (activator overlay
      appears over the primary monitor).
- [ ] Right-click menu contains: Show activator / Edit zones… / Layout ▶ /
      Pause (checkbox) / About gnome-zones.
- [ ] Layout submenu lists every layout from `ListLayouts`. Selecting one
      calls `AssignLayout primary_monitor layout_id`.
- [ ] After a "Save as…" in the editor, the layout submenu refreshes to
      include the new name.
- [ ] Pause toggle reflects `paused` setting. Toggling it calls
      `TogglePaused`; the tray icon's checkmark updates (via `PausedChanged`
      signal round-trip).
- [ ] Tray icon survives screen-lock (still present after unlock).
- [ ] Tray icon persists across daemon restarts:
      `systemctl --user restart gnome-zones-daemon` — icon stays visible;
      menu clicks continue to reach the (restarted) daemon after at most 1s
      of retry backoff.

## Multi-monitor

- [ ] Focus a window on the secondary monitor; fire `Super+Backquote`.
      Overlay covers **that** monitor, not the primary.
- [ ] Editor invoked from the secondary monitor edits that monitor's
      assignment. Apply assigns to the secondary monitor's `monitor_key`.
- [ ] Unplug the secondary display; re-plug — the previously-assigned layout
      is restored (daemon emits `MonitorsChanged`; tray doesn't need to
      react, next overlay invocation picks up fresh monitor data).

## Shutdown

- [ ] `pkill gnome-zones` — tray icon disappears cleanly within 1s. No
      zombie SNI registrations (verify with
      `busctl --user list | grep StatusNotifierItem`).
- [ ] Restarting `gnome-zones` re-registers the tray cleanly.

## Stress

- [ ] Fire `Super+Backquote` 10 times in 2 seconds — no crash, no zombie
      overlays stacked on top of each other.
- [ ] Editor with 9 zones performs a divider drag at 60 fps without visible
      lag.
- [ ] Leave `gnome-zones` running for >1 hour with periodic daemon restarts
      (`systemctl --user restart gnome-zones-daemon`); tray and signal
      subscriptions remain functional (verify by firing another
      `ShowActivator` after each restart).
