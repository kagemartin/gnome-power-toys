# gnome-clips Design Spec

**Date:** 2026-04-14  
**Project:** gnome-power-toys  
**Status:** Approved

---

## Overview

gnome-clips is a clipboard history manager for GNOME desktop (targeting Ubuntu). It mirrors and improves upon the Windows clipboard history feature (`Win+V`), providing persistent multi-format history, search, pinning, tagging, and privacy controls.

---

## 1. Architecture

Two Rust crates in a single Cargo workspace.

### `gnome-clips-daemon`

A background systemd user service (`~/.config/systemd/user/gnome-clips-daemon.service`) that:

- Monitors the clipboard using `wl-clipboard` (Wayland-native) with an XWayland fallback via `x11-clipboard`
- Persists history to SQLite at `~/.local/share/gnome-clips/history.db`
- Enforces the retention policy (default: 7 days, 100 items — both user-configurable)
- Owns the exclusion list and incognito state
- Exposes all functionality over D-Bus at bus name `org.gnome.Clips`, object path `/org/gnome/Clips`, on the session bus

### `gnome-clips` (UI)

A GTK4/libadwaita UI process that:

- Starts on demand when the keyboard shortcut fires or the panel icon is clicked
- Connects to the daemon over D-Bus
- Presents the two-pane popup (filter bar + scrollable list / preview pane)
- Registers the panel status icon via `libayatana-appindicator3`
- Registers the `Super+V` keyboard shortcut (configurable) via `org.gnome.settings-daemon.plugins.media-keys` custom shortcuts through `gsettings`

### Workspace layout

```
gnome-power-toys/
├── Cargo.toml                  # workspace
├── crates/
│   ├── gnome-clips-daemon/     # background service
│   └── gnome-clips/            # GTK4 UI
├── dist/
│   ├── flatpak/                # Flatpak manifest + modules
│   └── debian/                 # .deb packaging
└── docs/
    └── superpowers/specs/
```

---

## 2. Content Types

The following clipboard content types are captured and stored:

| Type | MIME | Notes |
|---|---|---|
| Plain text | `text/plain` | Default text |
| HTML | `text/html` | Rich text from browsers, editors |
| Markdown | `text/markdown` | From Obsidian, Typora, etc. |
| PNG image | `image/png` | Screenshots, copied images |
| JPEG image | `image/jpeg` | |
| File reference | `application/file` | Copied files (path + metadata) |

---

## 3. Data Model

SQLite database at `~/.local/share/gnome-clips/history.db`.

```sql
CREATE TABLE clips (
    id           INTEGER PRIMARY KEY,
    content      BLOB    NOT NULL,         -- raw bytes for all types
    content_type TEXT    NOT NULL,         -- MIME type string
    preview      TEXT,                     -- truncated text preview for list display
    source_app   TEXT,                     -- Wayland app-id or WM_CLASS of source window
    created_at   INTEGER NOT NULL,         -- Unix timestamp
    pinned       INTEGER NOT NULL DEFAULT 0,
    deleted      INTEGER NOT NULL DEFAULT 0  -- soft delete (hard-deleted by retention job)
);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE clip_tags (
    clip_id INTEGER REFERENCES clips(id),
    tag_id  INTEGER REFERENCES tags(id),
    PRIMARY KEY (clip_id, tag_id)
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
    -- keys: retention_days, retention_count, shortcut_key, incognito
);

CREATE TABLE exclusions (
    app_id TEXT PRIMARY KEY  -- e.g. "org.keepassxc.KeePassXC"
);
```

### Retention

A cleanup job runs on daemon startup and every hour. It hard-deletes non-pinned clips that exceed either `retention_days` (default: 7) or `retention_count` (default: 100), whichever triggers first. Pinned clips are exempt from both limits.

User-initiated deletes (`DeleteClip` D-Bus method) are **immediate hard deletes** — the row is removed from the database at once, not soft-deleted. The `deleted` column exists only as a write-ahead tombstone for in-flight UI refreshes (set to `1`, then row is removed within the same transaction).

### Preview generation

The `preview` column holds a generated text summary used for fast list rendering without deserialising the blob:

- Text / HTML / Markdown: first 200 characters of content
- Image: `[Image WxH]` (e.g. `[Image 1920×1080]`)
- File: `[File: filename.ext]`

---

## 4. D-Bus Interface

**Bus name:** `org.gnome.Clips`  
**Object path:** `/org/gnome/Clips`

### Methods

| Method | Arguments | Returns |
|---|---|---|
| `GetHistory` | `filter: &str, search: &str, offset: u32, limit: u32` | `Vec<ClipSummary>` — `filter` is one of: `""` (all), `"text/plain"`, `"text/html"`, `"text/markdown"`, `"image/*"`, `"application/file"`, `"pinned"` |
| `GetClip` | `id: i64` | `ClipDetail` |
| `DeleteClip` | `id: i64` | `()` |
| `SetPinned` | `id: i64, pinned: bool` | `()` |
| `AddTag` | `id: i64, tag: &str` | `()` |
| `RemoveTag` | `id: i64, tag: &str` | `()` |
| `SetIncognito` | `enabled: bool` | `()` |
| `GetSettings` | `()` | `HashMap<String, String>` |
| `SetSetting` | `key: &str, value: &str` | `()` |
| `AddExclusion` | `app_id: &str` | `()` |
| `RemoveExclusion` | `app_id: &str` | `()` |

### Signals

| Signal | Arguments |
|---|---|
| `ClipAdded` | `ClipSummary` |
| `ClipDeleted` | `id: i64` |
| `ClipUpdated` | `ClipSummary` |
| `IncognitoChanged` | `enabled: bool` |

### Types

**`ClipSummary`** (used for list rendering — no blob transfer):
```
(id: i64, content_type: String, preview: String, source_app: String,
 created_at: i64, pinned: bool, tags: Vec<String>)
```

**`ClipDetail`** (fetched on item selection or paste):
```
ClipSummary + content: Vec<u8>
```

---

## 5. UI Design

### Trigger

- **Keyboard shortcut:** `Super+V` (default). Registered via `org.gnome.settings-daemon.plugins.media-keys` custom shortcuts. In v1, the shortcut is changed via `gsettings` CLI or `dconf-editor` — no in-app Settings UI exists yet. The daemon stores the configured shortcut key in the `settings` table; the UI reads it on startup to register with GNOME.
- **Panel icon:** `libayatana-appindicator3` status icon in the system tray. Left-click opens the popup; right-click shows a menu with Incognito toggle and Settings.

### Popup layout

A floating window (not fullscreen) that appears centered on the active monitor.

```
┌─────────────────────────────────────────────────────────────┐
│ [🔍 Search clipboard history…] [All][Text][Image][File]     │
│                                 [HTML][MD][📌 Pinned]        │
├──────────────────────────┬──────────────────────────────────┤
│ 📌 PINNED                │  Text clip                       │
│  ▌npm install --save-dev │  gedit · just now · #work        │
│                          │                                  │
│ RECENT                   │  ┌──────────────────────────┐   │
│ ▶ Hello from gnome-clip… │  │ Hello from gnome-clips!  │   │
│   [Image — 1920×1080]    │  │ This is a longer piece…  │   │
│   <h1>Welcome</h1>…      │  └──────────────────────────┘   │
│   report-final-v2.pdf    │                                  │
│   # My Doc\n\nSome **…   │  Tags: [work] [+ add tag]        │
│                          │                                  │
│                          │  [📌 Pin] [🗑 Delete] [⏎ Paste] │
├──────────────────────────┴──────────────────────────────────┤
│ 47 items · 6 pinned · 7 days    ⎋ close · ↑↓ nav · ⏎ paste │
└─────────────────────────────────────────────────────────────┘
```

**Left pane:** Scrollable clip list. Pinned clips appear in a dedicated section above recent items. Each item shows a type icon, truncated preview, source app, and relative timestamp. A `✕` button on hover deletes the item. The selected item is highlighted with a blue left-border accent; pinned items use a green accent.

**Right pane:** Full content preview. For text/HTML/Markdown: monospaced content area. For images: rendered image. For files: file icon + metadata. Below the preview: tag chips, Pin/Delete/Paste action buttons.

**Filter bar:** Pill buttons for All / Text / Image / File / HTML / MD / Pinned. Active filter highlighted. Filters and search combine (AND logic).

**Status bar:** Item count, pinned count, retention setting on the left. Keyboard hint summary on the right.

---

## 6. Keyboard Navigation

The popup is fully keyboard-operable:

| Key | Action |
|---|---|
| `↑` / `↓` | Move through clip list (wraps) |
| `Enter` | Paste selected clip and close popup |
| `Tab` | Move focus between filter pills |
| `←` / `→` (on filter bar) | Cycle content-type filters |
| `Ctrl+F` or typing | Focus search box |
| `Escape` | Close popup without pasting |
| `Delete` | Delete selected clip |
| `Ctrl+P` | Toggle pin on selected clip |
| `Ctrl+T` | Add tag to selected clip |
| `Ctrl+I` | Toggle incognito mode |
| `1`–`9` | Paste the Nth item directly |

The list scrolls automatically to keep the selected item in view. Navigation wraps at both ends of the list.

---

## 7. Privacy & Exclusion

### App exclusion list

The daemon reads `source_app` (Wayland `app-id` or `WM_CLASS`) from each clipboard event. If the source app is in the exclusion list, the clip is silently discarded — never written to disk, never surfaced to the UI.

Default exclusion list:
- `org.keepassxc.KeePassXC`
- `com.1password.1password`
- `com.bitwarden.desktop`

Users can add/remove entries via Settings or via the D-Bus `AddExclusion`/`RemoveExclusion` methods.

### Incognito mode

A boolean D-Bus property `IsIncognito`. When enabled, the daemon discards all clipboard events without storing them. Togglable via:
- `Ctrl+I` in the popup
- Panel icon right-click menu
- D-Bus `SetIncognito` method

Visual indicators when incognito is active:
- Lock badge on the panel icon
- "🔒 Recording paused" indicator in the popup header (replaces "🔒 Recording on")

### Logging policy

The daemon never logs clip content to stdout, stderr, or any log file. Only metadata (timestamps, content type, source app) appears in structured logs.

---

## 8. Packaging

### Flatpak (`org.gnome.Clips`)

- Sandboxed — uses `xdg-portal` for clipboard access (`org.freedesktop.portal.Clipboard`)
- `gnome-clips-daemon` runs as a background portal consumer
- `gnome-clips` UI runs as the foreground app
- Auto-start via the Background portal (`org.freedesktop.portal.Background`)
- Manifest and module definitions in `dist/flatpak/`

### Debian package (`.deb`)

- Installs both binaries to `/usr/bin/`
- Installs systemd user service unit to `/usr/lib/systemd/user/gnome-clips-daemon.service`
- Registers the default keyboard shortcut via a gsettings override in `/usr/share/glib-2.0/schemas/`
- Post-install script: `systemctl --user enable --now gnome-clips-daemon`
- Packaging files in `dist/debian/`

Both packages are built from the same Rust workspace. CI produces both artifacts on each release tag.

---

## 9. Platform Support

**Wayland** is the primary target (Ubuntu 22.04+ defaults to Wayland). Clipboard monitoring uses `wl-clipboard` via the `wl-clipboard-rs` crate.

**X11 fallback:** On a pure X11 session (detected via `$WAYLAND_DISPLAY` absence), the daemon falls back to `x11-clipboard`. Panel icon and keyboard shortcut registration work identically. X11 support is best-effort — tested but not the primary development target.

**XWayland apps** (e.g. legacy GTK2/Qt5 apps running under XWayland) are handled transparently by the Wayland clipboard protocol — no special casing required.

---

## 10. Out of Scope

The following are explicitly out of scope for this initial version:

- Cloud sync (no Microsoft account equivalent)
- Cross-device sharing
- End-to-end encrypted history
- A dedicated Settings window (settings accessible via D-Bus / gsettings only in v1)
- GNOME Shell extension component
