# gnome-zones Design Spec

**Date:** 2026-04-14
**Project:** gnome-power-toys
**Status:** Approved

---

## Overview

gnome-zones is a window-zone manager for GNOME desktop (targeting Ubuntu). It mirrors and adapts the Windows PowerToys FancyZones feature: the user defines named layouts of rectangular zones on each monitor, then snaps focused windows into those zones via keyboard shortcuts. v1 is keyboard-driven; drag-to-snap is deferred to v2 (it requires a window-drag-intercepting GNOME Shell extension).

---

## 1. Architecture

Two Rust crates added to the existing `gnome-power-toys` Cargo workspace, mirroring the `gnome-clips` pattern.

### `gnome-zones-daemon`

A background systemd user service (`~/.config/systemd/user/gnome-zones-daemon.service`) that:

- Holds the authoritative zone model and persists it to SQLite at `~/.local/share/gnome-zones/zones.db`
- Owns the global hotkey grabs (registered through `org.gnome.settings-daemon.plugins.media-keys` custom shortcuts)
- Performs window move-resize via `org.gnome.Mutter.DisplayConfig`, with a Shell-extension shim fallback (`gnome-zones-mover@power-toys`) for installations where the Mutter API path is unavailable
- Subscribes to `org.gnome.Mutter.DisplayConfig.MonitorsChanged` for hot-plug awareness
- Exposes all functionality over D-Bus at bus name `org.gnome.Zones`, object path `/org/gnome/Zones`, on the session bus

### `gnome-zones` (UI)

A GTK4/libadwaita UI process that runs on demand. It hosts:

- The full-screen **zone editor** overlay (invoked via `Super+Shift+E` or panel-icon menu)
- The transient **activator overlay** (invoked via `Super+Backquote` — numbered zones for snap selection)
- The panel status icon via `libayatana-appindicator3`

Both overlays use `gtk4-layer-shell` on Wayland and `_NET_WM_WINDOW_TYPE_DOCK` on X11.

### Workspace layout

```
gnome-power-toys/
├── Cargo.toml                    # workspace
├── crates/
│   ├── gnome-clips-daemon/       # existing
│   ├── gnome-clips/              # existing
│   ├── gnome-zones-daemon/       # new
│   └── gnome-zones/              # new
├── dist/
│   ├── flatpak/                  # Flatpak manifests
│   ├── debian/                   # .deb packaging
│   └── shell-extension/          # gnome-zones-mover@power-toys (JS shim)
└── docs/
    └── superpowers/specs/
```

---

## 2. Data Model

SQLite database at `~/.local/share/gnome-zones/zones.db`. Coordinates are **fractional (0.0–1.0)** so a layout adapts to any monitor resolution. Monitor identity is multi-monitor-aware from day one (per the v1 scope decision: data model multi-monitor, UX single-monitor).

```sql
-- A named layout someone designed (e.g. "2/3 + 1/3", "Coding 4-pane")
CREATE TABLE layouts (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL,
    is_preset    INTEGER NOT NULL DEFAULT 0,    -- 1 = built-in (read-only), 0 = user
    created_at   INTEGER NOT NULL
);

-- The rectangles inside a layout. Coordinates are FRACTIONAL (0.0–1.0).
CREATE TABLE zones (
    id          INTEGER PRIMARY KEY,
    layout_id   INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE,
    zone_index  INTEGER NOT NULL,        -- 1-based, used by Super+Ctrl+N hotkey
    x           REAL    NOT NULL,        -- 0.0 = monitor left edge
    y           REAL    NOT NULL,        -- 0.0 = monitor top edge
    w           REAL    NOT NULL,        -- 0.0–1.0 of monitor width
    h           REAL    NOT NULL,        -- 0.0–1.0 of monitor height
    UNIQUE (layout_id, zone_index)
);

-- Which layout is active on which monitor.
-- monitor_key = "<connector>:<edid_hash>" (e.g. "DP-1:a1b2c3d4")
CREATE TABLE monitor_assignments (
    monitor_key  TEXT    PRIMARY KEY,
    layout_id    INTEGER NOT NULL REFERENCES layouts(id),
    updated_at   INTEGER NOT NULL
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
    -- keys: paused, gap_px, activator_timeout_ms,
    --       hotkey_snap_n, hotkey_span_n, hotkey_activator,
    --       hotkey_iterate_prev, hotkey_iterate_next,
    --       hotkey_cycle_next, hotkey_cycle_prev,
    --       hotkey_editor, hotkey_pause,
    --       gnome_default_tile_left, gnome_default_tile_right (stashed)
);
```

### Default presets (seeded on first run, `is_preset = 1`)

| Preset name | Zone count |
|---|---|
| Two Columns (50/50) | 2 |
| Three Columns | 3 |
| 2/3 \| 1/3 | 2 |
| 1/3 \| 2/3 | 2 |
| 2×2 Grid | 4 |
| 1/3 \| 1/3 \| 1/3 | 3 |
| Sidebar + Main (1/4 \| 3/4) | 2 |
| Main + Sidebar (3/4 \| 1/4) | 2 |

### Indexed iteration

`Super+Left` / `Super+Right` iterate the focused window through zones **by `zone_index`**, in the row-major reading order the editor enforces. `Super+Right` moves to `(current_index % zone_count) + 1`; `Super+Left` moves to `((current_index - 2 + zone_count) % zone_count) + 1`. Both wrap. If the window isn't currently snapped, the first press snaps it to zone 1 (Right) or the last zone (Left).

Indexed iteration keeps the key mapping predictable regardless of layout geometry and avoids overriding `Super+Up` (maximize) and `Super+Down` (unmaximize), which remain GNOME defaults.

### Gap

`gap_px` is a global setting (default 8) — visual breathing room between snapped windows. Subtracted from each zone rect on all sides when computing the move-resize target.

---

## 3. D-Bus Interface

**Bus name:** `org.gnome.Zones`
**Object path:** `/org/gnome/Zones`
**Bus:** session

### Methods (called by the UI)

| Method | Arguments | Returns |
|---|---|---|
| `ListLayouts` | `()` | `Vec<LayoutSummary>` |
| `GetLayout` | `id: i64` | `Layout` |
| `CreateLayout` | `name: &str, zones: Vec<ZoneRect>` | `i64` |
| `UpdateLayout` | `id: i64, name: &str, zones: Vec<ZoneRect>` | `()` |
| `DeleteLayout` | `id: i64` | `()` (rejected if `is_preset = 1`) |
| `ListMonitors` | `()` | `Vec<MonitorInfo>` |
| `AssignLayout` | `monitor_key: &str, layout_id: i64` | `()` |
| `GetActiveLayout` | `monitor_key: &str` | `Layout` |
| `ShowActivator` | `()` | `()` |
| `GetSettings` | `()` | `HashMap<String, String>` |
| `SetSetting` | `key: &str, value: &str` | `()` |

### Methods (invoked by hotkey handlers; also reachable for testing)

| Method | Arguments | Returns |
|---|---|---|
| `SnapFocusedToZone` | `zone_index: u32, span: bool` | `()` |
| `IterateFocusedZone` | `direction: String` | `()` (`"prev"` or `"next"`) |
| `CycleFocusInZone` | `direction: i32` | `()` (`+1` next, `-1` previous) |
| `TogglePaused` | `()` | `()` |

### Signals

| Signal | Arguments |
|---|---|
| `LayoutsChanged` | `()` |
| `LayoutAssigned` | `monitor_key: String, layout_id: i64` |
| `MonitorsChanged` | `()` |
| `PausedChanged` | `paused: bool` |
| `ActivatorRequested` | `monitor_key: String` |
| `EditorRequested` | `monitor_key: String` |

### Types

```
ZoneRect       = (zone_index: u32, x: f64, y: f64, w: f64, h: f64)
LayoutSummary  = (id: i64, name: String, is_preset: bool, zone_count: u32)
Layout         = (id: i64, name: String, is_preset: bool, zones: Vec<ZoneRect>)
MonitorInfo    = (monitor_key: String, connector: String, name: String,
                  width_px: u32, height_px: u32, is_primary: bool)
```

---

## 4. Hotkey Scheme & Window Snapping

### Default hotkeys (all configurable via `settings` table)

| Shortcut | Action |
|---|---|
| `Super+Ctrl+1` … `Super+Ctrl+9` | Snap focused window to zone N |
| `Super+Alt+1` … `Super+Alt+9` | Add zone N to current window's span |
| `Super+Backquote` | Show activator overlay |
| `Super+Left` / `Super+Right` | Iterate focused window to previous / next zone by index (wraps) |
| `Super+Page_Up` / `Super+Page_Down` | Cycle focus between windows in the same zone |
| `Super+Shift+E` | Open zone editor for the current monitor |
| `Super+Shift+P` | Pause/resume gnome-zones |

### Conflict with GNOME defaults

`Super+Left` / `Super+Right` are GNOME's built-in tile-half-screen shortcuts. The daemon **takes over** these two bindings on first run:

1. Read `org.gnome.mutter.keybindings.toggle-tiled-left` and `toggle-tiled-right`, **stash** their values in our `settings` table (keys `gnome_default_tile_left`, `gnome_default_tile_right`)
2. Unbind via `gsettings set org.gnome.mutter.keybindings toggle-tiled-left "[]"` (and `-right`)
3. Register our `Super+Left` / `Super+Right` via the `org.gnome.settings-daemon.plugins.media-keys` custom-keybindings list
4. On uninstall, or if the user disables zone navigation in `settings`, restore the stashed values

`Super+Up` (maximize) and `Super+Down` (unmaximize) are **left alone** — we chose indexed iteration on a single axis precisely to avoid those conflicts. Users who want a maximize-equivalent can snap to a full-screen preset zone (`Super+Ctrl+1` when zone 1 covers the full monitor).

### Snap behavior

When `SnapFocusedToZone(zone_index, span = false)` fires:

1. Resolve focused window's monitor → look up active layout for that `monitor_key`
2. If the focused window is fullscreen or maximized, un-maximize first
3. Compute target rect: `(zone.x * monitor.w, zone.y * monitor.h, zone.w * monitor.w, zone.h * monitor.h)`, then deflate by `gap_px` on all four sides
4. Issue move-resize via `org.gnome.Mutter.DisplayConfig` (or via the Shell-extension shim — see §7)
5. Record the window's pre-snap rect in an in-memory `HashMap<window_id, Rect>` (used internally; surfaced as a UI feature in v2)
6. Record the window's current zone-set in `HashMap<window_id, Vec<u32>>` for span tracking

### Span behavior

When `SnapFocusedToZone(zone_index, span = true)` fires (i.e. `Super+Alt+N`):

- If the window has a tracked current zone-set, compute the **bounding rectangle** of that set ∪ `{zone_index}`, deflate by gap, move-resize
- If not currently snapped, treat as a single-zone snap (`span = false`)
- Update the in-memory span map

### Indexed iteration (`Super+Left` / `Super+Right`)

1. Resolve the focused window's current `zone_index` on its monitor. If the window isn't currently tracked in the span map, treat its `zone_index` as `0` so the first Right press lands on zone 1 and the first Left press lands on the last zone
2. Compute next index: `Super+Right` → `(current % zone_count) + 1`; `Super+Left` → `((current - 2 + zone_count) % zone_count) + 1`
3. Snap the focused window to that zone (single-zone snap, updates span map to that one zone)

### Focus cycling (`Super+PgUp/PgDn`)

1. Identify the current zone of the focused window
2. Build the list of all top-level windows whose center lies within that zone's rect (excluding minimized)
3. Sort by `_NET_CLIENT_LIST_STACKING` order
4. Activate the next (`+1`) or previous (`-1`) window in that list

---

## 5. Zone Editor (Full-screen Overlay)

When triggered (`Super+Shift+E` or "Edit zones…" panel-menu item), the daemon emits `EditorRequested` carrying the active monitor's `monitor_key`. The UI process spawns and shows a borderless GTK4 window covering that monitor, layered above the desktop.

### Visual structure

```
┌────────────────────────────────────────────────────────────────────┐
│                                                                    │
│   ┌────────────────────────────┐  ┌──────────────────────────┐    │
│   │                            │  │                          │    │
│   │           Zone 1           │  │         Zone 2           │    │
│   │                            │  │                          │    │
│   │  • drag dividers to resize │  │                          │    │
│   │  • drag handles to split   │  ├──────────────────────────┤    │
│   │                            │  │                          │    │
│   │                            │  │         Zone 3           │    │
│   │                            │  │                          │    │
│   └────────────────────────────┘  └──────────────────────────┘    │
│                                                                    │
│   ╭──────────────────────────────────────────────────────────╮    │
│   │  Layout: ⌄ 2/3 │ 1/3 (preset)                            │    │
│   │  [+ New from current]  [Save as…]  [Reset]               │    │
│   │  ─────────────────────────────────────────────            │    │
│   │  [➕ Split horizontal]  [➕ Split vertical]  [🗑 Delete]   │    │
│   │  Gap: ─●──── 8px    Zones: 3                             │    │
│   │  ─────────────────────────────────────────────            │    │
│   │  [✓ Apply]  [✕ Cancel]                       ⎋ to close  │    │
│   ╰──────────────────────────────────────────────────────────╯    │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

The body is a translucent dark backdrop (~85% opacity over the desktop). Zones render as semi-transparent blue rectangles with a 2px outline; dividers are 6px-wide draggable strips; a centered libadwaita `AdwClamp` toolbar palette docks at the bottom.

### Editor interactions

| Action | Effect |
|---|---|
| Click a zone | Selects it (orange outline) — toolbar context updates |
| Drag a divider | Resizes adjacent zones (snap to 5% increments while `Ctrl` held) |
| `+ Split horizontal` | Splits the selected zone into top/bottom halves |
| `+ Split vertical` | Splits the selected zone into left/right halves |
| `🗑 Delete` | Removes selected zone, merges its area into the larger neighbor |
| Click-drag in empty (unzoned) space | Draws a new zone from scratch |
| Layout dropdown | Switches to a different layout (preset or user-saved) |
| `+ New from current` | Forks the current layout into a new editable user copy |
| `Save as…` | Prompts (`AdwMessageDialog`) for a name; saves as user layout |
| `Reset` | Discards in-progress edits, reloads layout from DB |
| `Apply` | Persists changes, assigns layout to the editor's monitor, closes |
| `Cancel` / `Esc` | Closes without saving |

### Editing presets

Built-in presets are read-only. Choosing **Save as…** or **+ New from current** while a preset is loaded automatically forks it into an editable user layout. The preset itself never changes.

### Re-numbering on edit

When zones are added, removed, or reordered, `zone_index` is renumbered in **row-major reading order** (top-to-bottom, left-to-right, by top-left corner). The new numbers display live as a large semi-transparent number in each zone's corner so the user knows what `Super+Ctrl+N` will hit.

### Multi-monitor in v1

The editor only operates on the monitor the user invoked it from. The data model already keys layouts per `monitor_key`; v2 will add a per-monitor editor switcher without a schema migration.

---

## 6. Activator Overlay

When `Super+Backquote` (or `ShowActivator` D-Bus method) fires, the daemon emits `ActivatorRequested` carrying the focused window's `monitor_key`. The UI process spawns a transparent overlay covering that monitor for ~3 seconds.

### Visual

Each zone of the active layout renders as:

- Semi-transparent fill (blue, ~25% alpha)
- 2px solid outline
- Large centered number (`zone_index`) in the system accent color, ~96pt
- Optional zone name (caption beneath the number) if one was set in the editor

The backdrop itself is fully transparent — only the zones are visible.

### Interactions

| Key | Action |
|---|---|
| `1` … `9` | Snap focused window to that zone, dismiss overlay |
| `Shift+1` … `Shift+9` | Snap and keep overlay open (lets the user span by pressing a second number) |
| `Esc` | Dismiss without snapping |
| Any other key | Dismiss without snapping |
| Mouse click on a zone | Snap focused window to that zone |

The overlay auto-dismisses after `activator_timeout_ms` (default 3000) without input.

### Critical implementation note

The overlay must **not steal focus** from the originally-focused window. Uses `gtk4-layer-shell` (Wayland) with `KeyboardInteractivity::OnDemand`, or `_NET_WM_WINDOW_TYPE_DOCK` (X11). The daemon caches the focused window's id when it receives the activator request, and uses that cached id for the snap — never re-resolving focus once the overlay is up.

---

## 7. Panel Icon, Pause/Resume, Monitor Hot-plug

### Panel status icon

`libayatana-appindicator3` icon in the system tray:

- **Default icon:** outlined zone-grid glyph (a 2×2 grid of small rounded rectangles)
- **Paused icon:** same glyph with a strikethrough
- **Left-click:** triggers `ShowActivator` for the current monitor
- **Right-click menu:**
  - "Edit zones…" → triggers `EditorRequested`
  - "Layout: ⌄ <current name>" → submenu lists all layouts; selecting one calls `AssignLayout` for the current monitor
  - separator
  - "Pause" / "Resume" → `TogglePaused`
  - "About gnome-zones"

### Pause behavior

When `paused = 1`:

- All snap and navigation hotkey handlers (`SnapFocusedToZone`, `IterateFocusedZone`, `CycleFocusInZone`) become no-ops
- The activator overlay still opens on `Super+Backquote` but shows a "Paused" banner instead of zone numbers; pressing `1`-`9` is ignored
- The editor remains fully accessible (`Super+Shift+E`) — pause suppresses runtime snapping only, not configuration

### Monitor hot-plug and reconfiguration

The daemon subscribes to `org.gnome.Mutter.DisplayConfig.MonitorsChanged`. On each event:

1. Re-enumerate monitors → recompute `monitor_key` (connector + EDID hash) for each
2. **New monitor with no assignment:** auto-assign the layout named "Two Columns (50/50)" (the safe default)
3. **Removed monitor:** `monitor_assignments` row stays in the DB. If the same monitor is re-attached later, its layout is restored automatically
4. **Resolution change on existing monitor:** no action needed — fractional coordinates are resolution-independent
5. Emit `MonitorsChanged` D-Bus signal for any open UI

### Window edge cases

- **Window destroyed while in span/pre-snap maps:** Daemon listens for window-destroyed events (X11 `_NET_WM_ICCCM`, Wayland `wl_surface` destruction) and prunes both maps
- **Window manually moved to another monitor:** No action; next snap resolves against the new monitor's active layout
- **Focused window is fullscreen/maximized:** Snap and iterate handlers un-maximize first

### Window move-resize fallback (Shell-extension shim)

GNOME Shell on Wayland doesn't expose a stable, sandboxed API for arbitrary apps to move/resize *other* apps' windows. The daemon uses `org.gnome.Mutter.DisplayConfig` where available; on installations without that, it falls back to a tiny GNOME Shell extension shim:

- **Extension id:** `gnome-zones-mover@power-toys`
- **Surface:** D-Bus method `MoveResizeWindow(window_id: u64, x: i32, y: i32, w: i32, h: i32)` on `org.gnome.Shell.Extensions.GnomeZonesMover`
- **Logic:** none — pure Mutter `Meta.Window.move_resize_frame()` bridge
- **Shipping:** packaged in `dist/shell-extension/`, installed by the .deb to `/usr/share/gnome-shell/extensions/gnome-zones-mover@power-toys/`, auto-enabled on first run via `gnome-extensions enable`

This is **not** the drag-to-snap extension (that's deferred to v2); it's strictly the move-resize bridge.

---

## 8. Platform Support

**Wayland (Ubuntu 22.04+ default)** — primary target. Uses:

- `gtk4-layer-shell` for editor and activator overlays
- `org.gnome.Mutter.DisplayConfig` for monitor enumeration and window move-resize
- `org.gnome.settings-daemon.plugins.media-keys` custom-keybindings for global hotkeys
- The `gnome-zones-mover` Shell extension shim where Mutter's D-Bus path is unavailable

**X11 fallback** — best-effort, tested but not the primary development target. Window move-resize uses `xcb` directly (no Shell extension required on X11). Overlays use `_NET_WM_WINDOW_TYPE_DOCK`. Hotkey grabs use `xcb` `GrabKey`.

---

## 9. Packaging

### Flatpak (`org.gnome.Zones`)

- Sandboxed; `gnome-zones-daemon` runs as a background portal consumer (Background portal for auto-start)
- `gnome-zones` UI runs as the foreground app
- Manifest in `dist/flatpak/`
- The Shell-extension shim is shipped as a separate download referenced from the Flatpak metadata; user grants permission on first run (one-shot prompt)

### Debian package (`.deb`)

- Installs both binaries to `/usr/bin/`
- Installs systemd user unit to `/usr/lib/systemd/user/gnome-zones-daemon.service`
- Installs the Shell-extension shim to `/usr/share/gnome-shell/extensions/gnome-zones-mover@power-toys/`
- Registers default hotkeys via gsettings override in `/usr/share/glib-2.0/schemas/`
- Post-install: `systemctl --user enable --now gnome-zones-daemon` + `gnome-extensions enable gnome-zones-mover@power-toys`

Both packages are built from the same Rust workspace. CI produces both artifacts on each release tag (matching the `gnome-clips` pipeline).

---

## 10. Testing Strategy

- **Unit tests** in each crate cover: zone-rect math, fractional→pixel projection, gap deflation, span bounding-rect computation, indexed iteration (wrap-around and unsnapped-start behavior), preset seeding
- **Integration tests** in `gnome-zones-daemon` use a mock `DisplayConfig` D-Bus service (spawned per test) to assert the daemon issues the right `MoveResizeWindow` calls in response to hotkey events
- **UI tests** for the editor use GTK4's headless test runner to verify divider drag math and split/delete operations
- **Manual smoke checklist** at `dist/test/manual.md` for the bits machines can't verify: visual overlay alpha, hotkey-grab survival across screen-lock, multi-monitor with one disconnected mid-session

---

## 11. Out of Scope (deferred to v2)

- **Drag-to-snap with `Shift` modifier** (requires an input-intercepting Shell extension, distinct from the move-resize shim shipped in v1)
- **Per-app default zones** ("Firefox always lands in zone 2 on monitor 1")
- **In-app Settings GTK4 window** (settings via `gsettings` / `dconf-editor` only in v1)
- **Window-position memory UI** ("unsnap to original size" — daemon records `pre_snap_rect` internally but exposes no UI)
- **Workspace-aware zone layouts** (different zones per GNOME workspace)
- **Layout import/export** (sharing layouts between machines)
- **Editor on multiple monitors simultaneously** (one-monitor-at-a-time editor in v1; data model already supports the v2 lift)
