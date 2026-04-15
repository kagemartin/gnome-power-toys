# gnome-deck Design Spec

**Date:** 2026-04-14
**Project:** gnome-power-toys
**Status:** Approved

---

## Overview

gnome-deck is a multi-modal workspace host for GNOME (targeting Ubuntu). It is a tab-and-layout shell — a recursive `Pane → TabHost → Tab → (Client | Layout)` tree, capped at three levels of nesting — hosting pluggable **Client** components. v1 ships three Clients: a **Terminal** (VTE), an **Editor** (GtkSourceView + markdown preview), and a **Workspace Browser** (chrome drawer). A future **Web Frame** client slots into the same contract without core changes.

Positioning: GNOME-native chrome and discoverable GUI, with power-user features first-class — tab coloring, per-tab locking, saved layout templates, reversible "peek" keyboard navigation, persistent dirty state across restart for every Client, and an importer pipeline that consumes Base16, iTerm2, and Windows Terminal theme formats.

Workspaces are IDE-style: a workspace is identified by a root folder; each workspace has its own running process (one `gnome-deck` instance per workspace), many named layouts, and a library of shared layout templates.

---

## 1. Architecture

Single-process foreground app, per-workspace singleton. No systemd daemon — gnome-deck's chords are in-window and nothing needs to run when no window is open, unlike `gnome-zones` and `gnome-clips` which grab global hotkeys.

### 1.1 Workspace layout

Three new members in the existing `gnome-power-toys` Cargo workspace:

```
gnome-power-toys/
├── Cargo.toml                     # workspace
├── crates/
│   ├── gnome-clips-daemon/        # existing
│   ├── gnome-clips/               # existing
│   ├── gnome-zones-daemon/        # existing
│   ├── gnome-zones/               # existing
│   ├── gnome-deck-core/           # NEW: tree model, persistence, theme loaders
│   ├── gnome-deck-clients/        # NEW: Terminal, Editor, WorkspaceBrowser impls
│   └── gnome-deck/                # NEW: binary — GTK4 app, chrome, launch router
├── dist/
│   ├── flatpak/                   # org.gnome.Deck
│   └── debian/                    # .deb
└── docs/superpowers/specs/
```

- **`gnome-deck-core`** — pure Rust library, no GTK dependency. Layout tree types, JSON serialization, SQLite repositories, theme importer pipeline, keyboard-chord state machine, peek-mode state machine. Fully unit-testable headless.
- **`gnome-deck-clients`** — depends on GTK4 + VTE4 + GtkSourceView + (optional) WebKit6. Implements the `Client` trait for each content type.
- **`gnome-deck`** — binary. Wires `GApplication`, the launch router, the chrome, and dispatches to `-clients` widgets.

### 1.2 External dependencies

| Dep | Purpose |
|---|---|
| `gtk4-rs` (≥ 0.8) | GTK4 bindings |
| `libadwaita-rs` (≥ 1.6) | Chrome widgets (`AdwHeaderBar`, `AdwOverlaySplitView`, `AdwMessageDialog`, `AdwNavigationSplitView`) |
| `vte4` (≥ 0.76) | Terminal widget |
| `sourceview5` | Editor widget |
| `webkit6` (feature-flagged `markdown-preview`) | Markdown preview renderer; also the v2 WebFrame client |
| `pulldown-cmark` | Markdown → HTML |
| `rusqlite` (bundled feature) | SQLite persistence |
| `serde` / `serde_json` | Layout-tree JSON |
| `serde_yaml` | Base16 theme import |
| `plist` | iTerm2 `.itermcolors` import |
| `nucleo` | Fuzzy matcher (palette + quick open) |
| `ignore` | `.gitignore` parsing for file tree |
| `notify` | Filesystem watcher |
| `bitflags` | `Capabilities` |

### 1.3 Runtime paths

| Path | Purpose |
|---|---|
| `~/.local/share/gnome-deck/deck.db` | SQLite, shared by all running instances, writes scoped by `workspace_id` |
| `~/.local/share/gnome-deck/themes/` | User-authored / user-imported themes (file-watched) |
| `~/.local/share/gnome-deck/templates/*.json` | Exported layout templates |
| `~/.local/share/gnome-deck/dirty/<workspace_id>/<tab_id>.dirty` | Per-tab dirty-state sidecars |
| `~/.local/share/gnome-deck/integration/` | Shell integration snippets (bash, zsh, fish) |
| `~/.config/gnome-deck/config.toml` | Optional user config overrides |
| `~/.config/gnome-deck/style.css` | Custom chrome CSS (hot-reloaded) |

### 1.4 Launch router

Runs before `GApplication` registers its ID so per-workspace singleton semantics can resolve the target workspace:

```rust
fn main() -> ExitCode {
    let args = parse_args();
    let ws = resolve_workspace(&args);                         // §10a resolution rules
    let app_id = format!("org.gnome.Deck.ws{}", ws.path_hash); // 64-bit hex hash of canonical path
    let app = Application::new(Some(&app_id),
                               ApplicationFlags::HANDLES_OPEN | ApplicationFlags::NON_UNIQUE);
    app.register(None::<&Cancellable>)?;
    if app.is_remote() {
        app.activate_action("open-resource", Some(&args.to_variant()));
        return ExitCode::SUCCESS;
    }
    app.connect_activate(|_| restore_session_or_empty_window());
    app.connect_open(|_, files, _| route_resources(files));
    app.run().into()
}
```

The `Recent Workspaces` picker (no-args launch) runs as its own tiny singleton with `app_id = "org.gnome.Deck.picker"`, separate from any workspace instance.

---

## 2. Core Model

Types live in `gnome-deck-core` with no GTK dependency. The entire tree is `serde`-serializable for persistence and for reversible peek-mode snapshots.

### 2.1 Type hierarchy

```rust
pub const MAX_LAYOUT_DEPTH: u8 = 3;

pub struct Window {
    pub id: WindowId,                // UUID
    pub workspace_id: WorkspaceId,
    pub active_layout_id: LayoutId,  // which saved layout is currently displayed
    pub root: Layout,                // always present; depth = 1
}

pub struct Layout {
    pub id: LayoutId,
    pub depth: u8,                   // 1..=MAX_LAYOUT_DEPTH
    pub kind: LayoutKind,
}

pub enum LayoutKind {
    Single(Pane),                                        // 1 cell — degenerate
    Split { orientation: Orientation,                    // Horizontal | Vertical
            ratio: f32,                                  // divider, (0.05..0.95)
            a: Box<Pane>,
            b: Box<Pane> },
    Grid  { rows: u8, cols: u8,                          // equal-sized cells
            panes: Vec<Pane> },                          // row-major, len == rows*cols
}

pub struct Pane {
    pub id: PaneId,
    pub tabs: TabHost,               // invariant: every Pane has exactly one TabHost
}

pub struct TabHost {
    pub active: usize,               // 0..tabs.len()
    pub tabs: Vec<Tab>,              // invariant: len >= 1
    pub tab_bar_hidden: bool,        // single-tab panes may auto-hide if user opts in
}

pub struct Tab {
    pub id: TabId,
    pub title: String,               // user-editable (F2 / double-click); auto-sync from Client
    pub color: Option<TabColor>,     // 8 named colors + None; applies to any tab
    pub locked: bool,                // can't be closed; rename/recolor/move still allowed
    pub body: TabBody,
}

pub enum TabBody {
    Client(ClientHandle),            // leaf
    Layout(Layout),                  // recursion; depth = parent.depth + 1
}
```

### 2.2 Invariants

Enforced on construction and on JSON deserialization; violations return `Error::InvalidTree`:

1. Every `Pane` has exactly one `TabHost` with `tabs.len() >= 1`.
2. `Layout::depth` is `1` at the root and strictly increments with each `TabBody::Layout` level.
3. At `depth == MAX_LAYOUT_DEPTH`, no `Tab.body` may be `TabBody::Layout`. Attempting to split a pane at max depth returns `Error::MaxDepthReached`; the UI disables the "split" action in this case.
4. `Split.ratio ∈ (0.05, 0.95)`.
5. `Grid.rows * Grid.cols == Grid.panes.len()`.
6. `Tab.locked` blocks `close_tab` but permits rename / recolor / move / drag.
7. Exactly one `Tab` per `Window` is focused; the focus path (window → layout → pane → tabhost → tab → client) is derivable from a `(WindowId, TabId)` pair.

### 2.3 Layout kinds

Two kinds plus a degenerate Single cover the full feature set without bloating the tree:

- **`Split`** — one axis, two children, draggable fractional divider. Natural for asymmetric arrangements (2/3 editor + 1/3 terminal).
- **`Grid`** — uniform `N×M` cells. A 2×2 is `Grid { rows: 2, cols: 2 }` at depth 2, leaving one depth level in reserve. Without `Grid` a 2×2 would eat 2 nesting levels via nested binary splits, leaving no headroom at `MAX_LAYOUT_DEPTH = 3`.
- **`Single`** — 1-cell; only at `depth == 1` when a window has no splits yet.

Divider drag is restricted to `Split` (grids are uniform; resize a grid by changing `rows`/`cols`, not by dragging). If the user wants non-uniform cells they use nested `Split`s.

### 2.4 Tree operations

All mutations go through this API; the UI never mutates directly. Each returns a new tree (immutable-style) so peek-mode rollback is free.

| Op | Effect |
|---|---|
| `split_pane(pane_id, orientation, ratio)` | Replace `Pane` with `Split` Layout containing the original pane + a blank pane. Depth-checked. |
| `grid_pane(pane_id, rows, cols)` | Replace `Pane` with `Grid` Layout; original pane goes to `(0,0)`, others blank. Depth-checked. |
| `unsplit(pane_id)` | If pane's parent `Split`/`Grid` would have one non-blank cell, collapse to `Single`. Re-flatten upward if the collapse cascades. |
| `move_tab(src_tab_id, dst_tabhost_id, dst_index)` | Migrate `Tab` between `TabHost`s (same window or different). Source TabHost with 0 tabs after move triggers pane removal. |
| `close_tab(tab_id)` | Fails if `locked`. If last tab in pane, cascade pane removal. |
| `new_tab(pane_id, client_kind, resource_ref)` | Append tab with fresh Client. |
| `rename_tab(tab_id, title)` / `color_tab(tab_id, color)` / `set_locked(tab_id, bool)` | Self-explanatory. |
| `swap_panes(a, b)` | Within the same Layout; powers visual pane movement. |

### 2.5 Focus path and "top-level pane"

The **focus path** is the ordered list of (Layout, Pane) pairs from root to the focused leaf. "Top-level pane" = the root Layout's pane corresponding to the first step of that path. Keyboard nav primitives read this path; `Ctrl+Tab` cycles among root-Layout children, `Ctrl+Alt+Tab` cycles among children of the deepest Layout on the focus path.

---

## 3. Data Model

All state lives in one SQLite database at `~/.local/share/gnome-deck/deck.db`, opened by every running instance. Writes are scoped by `workspace_id` so per-workspace instances don't collide; the only shared-write surface is the `workspaces` table (touched on open / rename) and `themes` / `global_settings`.

### 3.1 Design choices

- **Layout trees are stored as JSON blobs** in `layouts.tree_json`, not normalized. The tree is edited holistically (split, grid, move tab) and rarely queried structurally. A single blob per layout is correct.
- **Client state lives inside the layout JSON** — no separate `tabs` table. Rename / color / lock rewrite the blob; at v1 workspace sizes (hundreds of tabs max) this is imperceptible.
- **A `schema_version` row** gates migrations from day one. Migration runner at startup compares and applies diffs.
- **WAL journal mode** (`PRAGMA journal_mode=WAL`) for concurrent readers across instances.

### 3.2 Schema

```sql
CREATE TABLE global_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
    -- keys: schema_version, default_theme_id, default_font_mono, default_font_ui,
    --       leader_key, tab_rename_confirm, new_tabs_locked_by_default,
    --       session_restore_on_launch, recent_workspaces_limit,
    --       terminal.scrollback_persist_lines, terminal.env_allowlist,
    --       editor.autosave_interval_sec,
    --       stashed_ctrl_alt_tab,
    --       key.<command_id> for every rebindable chord
);

CREATE TABLE workspaces (
    id              TEXT PRIMARY KEY,            -- UUID
    name            TEXT NOT NULL,               -- default = folder basename; user-editable
    root_path       TEXT NOT NULL UNIQUE,        -- canonical absolute path
    path_hash       TEXT NOT NULL UNIQUE,        -- 64-bit hex; used in D-Bus app ID
    pinned          INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    last_opened_at  INTEGER NOT NULL
);

CREATE TABLE windows (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    active_layout_id  TEXT NOT NULL REFERENCES layouts(id),
    geometry_x        INTEGER, geometry_y INTEGER,
    geometry_w        INTEGER, geometry_h INTEGER,
    is_maximized      INTEGER NOT NULL DEFAULT 0,
    drawer_width      INTEGER NOT NULL DEFAULT 260,  -- workspace browser drawer
    drawer_visible    INTEGER NOT NULL DEFAULT 1,
    last_active_at    INTEGER NOT NULL
);

CREATE TABLE layouts (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT REFERENCES workspaces(id) ON DELETE CASCADE,  -- NULL for templates
    name          TEXT NOT NULL,                -- e.g. "debug", "writing"
    is_template   INTEGER NOT NULL DEFAULT 0,
    is_builtin    INTEGER NOT NULL DEFAULT 0,   -- read-only built-in templates
    tree_json     TEXT NOT NULL,                -- serialized Layout; see §3.3
    tree_schema_v INTEGER NOT NULL DEFAULT 1,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX idx_layouts_ws  ON layouts(workspace_id) WHERE workspace_id IS NOT NULL;
CREATE INDEX idx_layouts_tpl ON layouts(is_template)  WHERE is_template = 1;

CREATE TABLE ssh_hosts (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT REFERENCES workspaces(id) ON DELETE CASCADE,  -- NULL = global
    alias         TEXT NOT NULL,
    hostname      TEXT NOT NULL,
    username      TEXT,
    port          INTEGER NOT NULL DEFAULT 22,
    identity_path TEXT,
    created_at    INTEGER NOT NULL
);

CREATE TABLE resource_links (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    label         TEXT NOT NULL,
    kind          TEXT NOT NULL,                -- "url" | "file" | "command"
    target        TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE themes (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    source_kind  TEXT NOT NULL,                 -- "base16" | "iterm2" | "wt" | "native"
    palette_json TEXT NOT NULL,                 -- canonical 16-palette + fg/bg/cursor/selection
    is_builtin   INTEGER NOT NULL DEFAULT 0,
    imported_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_themes_name ON themes(name);
```

### 3.3 Tree JSON format (`layouts.tree_json`, `tree_schema_v = 1`)

Serialization of a `Layout`, mirroring the Rust types from §2 with `#[serde(tag = "type")]` on the enums for forward-compat.

```json
{
  "id": "0d3e…-uuid",
  "depth": 1,
  "kind": {
    "type": "split",
    "orientation": "horizontal",
    "ratio": 0.55,
    "a": {
      "id": "…", "tabs": {
        "active": 0, "tab_bar_hidden": false,
        "tabs": [
          { "id":"…", "title":"main.rs", "color":null, "locked":false,
            "body": { "type":"client", "kind":"editor",
                      "state": {"file":"/abs/path/main.rs",
                                "cursor":{"line":42,"col":8},
                                "preview":"hidden"} } }
        ]
      }
    },
    "b": {
      "id": "…", "tabs": {
        "active": 3, "tab_bar_hidden": false,
        "tabs": [
          { "id":"…","title":"cc-1","color":null,"locked":false,
            "body":{"type":"client","kind":"terminal",
                    "state":{"cwd":"/abs/path","cols":120,"rows":32}} },
          { "id":"…","title":"tail-logs","color":"orange","locked":true,
            "body":{"type":"layout","layout":{
              "id":"…", "depth":2,
              "kind":{"type":"grid","rows":2,"cols":2,"panes":[ …4 panes… ]}
            }} }
        ]
      }
    }
  }
}
```

**Client `state` shapes (per `kind`):**

| Kind | `state` fields |
|---|---|
| `terminal` | `cwd`, `cols`, `rows`, `shell?`, `command_override?` |
| `editor` | `file`, `cursor: {line, col}`, `scroll_top`, `preview: "hidden"|"left"|"right"|"top"|"bottom"` |
| `workspace_browser` | `expanded: [node_path]`, `scroll_top` |
| `web_frame` (v2) | `url`, `zoom` |

### 3.4 Dirty-state sidecars

The JSON blob in `layouts.tree_json` holds only **clean state** — the minimum to recreate a Client from a cold start. Everything the user has **mutated but not committed to the underlying source** lives in a per-tab **dirty sidecar** on disk:

```
~/.local/share/gnome-deck/dirty/<workspace_id>/<tab_id>.dirty
```

`<tab_id>` is the tab's UUID from the tree; no DB row needed — existence of the file is the signal that the tab is dirty. On graceful exit, every Client flushes its dirty payload; on crash, files from the previous run are still there and get restored next launch.

Sidecar payloads per Client kind:

| Kind | Sidecar payload |
|---|---|
| `terminal` | `scrollback` (last N lines, default N = 10 000), `env` (filtered allowlist), `cwd_at_snapshot` |
| `editor` | `buffer_contents`, `base_mtime`, `cursor`, `selection` |
| `workspace_browser` | — (no runtime state beyond clean) |
| `web_frame` (v2) | `url`, `scroll`, `back_forward_stack` |

**Env capture for terminals** requires shell cooperation. Three modes, v1 ships all three:

- **Preferred**: the shell-integration snippets under `~/.local/share/gnome-deck/integration/` (sourced from the user's rc). Hooks `PROMPT_COMMAND`/`precmd` to emit an OSC that encodes a filtered env snapshot (default allowlist: `PATH PWD OLDPWD VIRTUAL_ENV CONDA_DEFAULT_ENV NODE_ENV`). Allowlist editable in settings.
- **Without integration**: `PWD` via OSC 7 only. Restore spawns a fresh shell in that cwd; the shell's rc re-establishes everything else.
- **User-defined**: integration allowlist editable; custom hook script paths supported.

On restore, a terminal dirty sidecar reopens with: new PTY in the saved cwd, allowlisted env vars re-exported (injected as `export KEY=value` lines swallowed silently if integration is loaded), scrollback **painted as a frozen transcript region** above the live prompt — visually dimmed, user can scroll into it, cannot rerun commands.

### 3.5 Dirty indicator & revert semantics

- Tab bar shows **●** before the title when the tab has a sidecar.
- Tooltip by kind: `"Unsaved changes"` (editor), `"Session state saved (scrollback, env, cwd)"` (terminal).
- **Revert (per tab)** — context-menu + Command Palette: deletes the sidecar, reloads Client from clean state. Confirm dialog: *"Revert <title>? Saved scrollback and/or unsaved changes will be lost."*
- **Revert all dirty (workspace)** — Command Palette: deletes every sidecar for the current workspace; single confirm listing affected tabs.
- **Save (editor only)** — `Ctrl+S` persists buffer, deletes the sidecar.
- Terminal sidecars never "save to source" — no source exists. They persist until revert, close (deletes), or explicit clear (`Ctrl+Shift+R`).

### 3.6 Size management

- Scrollback cap: `terminal.scrollback_persist_lines` (default 10 000, `0` = disable). Lines beyond the cap truncate from the top on flush.
- If any sidecar exceeds 5 MB, the tab shows a subtle size-warn icon and settings gain a "trim scrollback" action.
- Closing a locked tab is blocked (lock protects the *tab*); when a tab is closed normally, its sidecar is deleted as part of tab removal.

### 3.7 Migrations

`global_settings.schema_version` seeded to `1` on fresh install. On startup:

```rust
let current = read_schema_version()?;
for v in (current+1)..=LATEST {
    apply_migration(v)?;                          // idempotent, transactional
}
write_schema_version(LATEST)?;
```

Each future migration is a `.sql` file in `crates/gnome-deck-core/migrations/NNNN_description.sql`, shipped with the binary.

---

## 4. Client Contract

Extensibility-first. Every Client is defined by one GObject-bearing Rust type plus metadata registered with the core. Adding a new Client = new type + registration call + JSON `kind` tag. The host never switches on Client kinds in its own code paths.

### 4.1 The `Client` trait

```rust
pub trait Client: Send + 'static {
    // Lifecycle
    fn new(cx: &ClientContext, clean: &serde_json::Value,
           dirty: Option<&serde_json::Value>) -> Result<Self> where Self: Sized;

    fn kind_id(&self) -> &'static str;                  // "terminal", "editor", ...
    fn widget(&self) -> &gtk::Widget;                   // mounted by TabHost (or chrome slot)
    fn focus_widget(&self) -> Option<&gtk::Widget>;

    // State serialization
    fn clean_state(&self) -> serde_json::Value;
    fn dirty_state(&self) -> Option<serde_json::Value>;
    fn revert_to_clean(&mut self) -> Result<()>;

    // UI integration
    fn title_stream(&self) -> Option<Box<dyn TitleStream>>;
    fn status_stream(&self) -> Option<Box<dyn StatusStream>>;
    fn icon(&self) -> Option<&str>;

    // Commands & keys
    fn contribute_commands(&self, out: &mut CommandRegistry);
    fn handle_key(&mut self, ev: &KeyEvent) -> KeyDisposition;

    // Capabilities
    fn capabilities(&self) -> Capabilities { self.descriptor().capabilities }
    fn descriptor(&self) -> &'static ClientDescriptor;
}
```

`ClientContext` provides: the workspace's SQLite pool, the theme-token resolver, a `DirtyFlusher` for proactive persistence on significant edits, and a `CommandDispatcher` for emitting host-level commands.

### 4.2 `ClientDescriptor` + registry

Each Client type is described by one static `ClientDescriptor` and registered via an `inventory`-style compile-time collection.

```rust
pub struct ClientDescriptor {
    pub kind_id: &'static str,                          // stable JSON tag; never renamed
    pub display_name: &'static str,
    pub icon: &'static str,
    pub capabilities: Capabilities,
    pub resource_handlers: &'static [ResourceHandler],
    pub constructor: fn(&ClientContext, &Value, Option<&Value>) -> Result<Box<dyn Client>>,
}

register_client! {
    ClientDescriptor {
        kind_id: "terminal",
        display_name: "Terminal",
        icon: "utilities-terminal-symbolic",
        capabilities: Capabilities::ACCEPTS_FOCUS
                    | Capabilities::HAS_SCROLLBACK
                    | Capabilities::EMITS_TITLE,
        resource_handlers: &[
            ResourceHandler::directory("Open here (cd)", open_in_terminal_cwd),
            ResourceHandler::ssh_host("Connect",          open_ssh_session),
        ],
        constructor: TerminalClient::construct,
    }
}
```

Core exposes `iter_clients() -> impl Iterator<Item = &ClientDescriptor>` and `client_by_kind(id) -> Option<&ClientDescriptor>`. All host logic (the "New Tab" menu, Command Palette, resource-open menu, deserializer) is data-driven off these.

### 4.3 `Capabilities`

```rust
bitflags! {
    pub struct Capabilities: u32 {
        const ACCEPTS_FOCUS       = 1 << 0;
        const HAS_SCROLLBACK      = 1 << 1;
        const EDITS_RESOURCE      = 1 << 2;             // enables Ctrl+S, dirty-as-unsaved
        const EMITS_TITLE         = 1 << 3;
        const SPLITTABLE          = 1 << 4;             // can be duplicated to a sibling pane
        const CONSUMES_ARROW_KEYS = 1 << 5;             // host avoids Arrow-based shortcuts near it
        const IS_CHROME           = 1 << 6;             // mounted in a chrome slot, not a tab
    }
}
```

Host logic consults these to enable/disable actions. New capability = additive flag + whatever reads it; existing clients stay source-compatible.

### 4.4 `CommandRegistry`

```rust
pub struct Command {
    pub id: &'static str,                               // "editor.save-as", "terminal.clear"
    pub label: &'static str,
    pub keywords: &'static [&'static str],
    pub accelerator: Option<&'static str>,
    pub context: Context,                               // Palette | ContextMenu | Toolbar | All
    pub enabled: Rc<dyn Fn(&AppState) -> bool>,
    pub execute: Rc<dyn Fn(&mut AppState) -> Result<()>>,
}
```

`contribute_commands` runs on mount + on focus; deregistration on unfocus/unmount. Host commands live in the same registry — same scoring, same conflict checker.

### 4.5 Key routing

Single `GtkEventControllerKey` at the `ApplicationWindow`, dispatched as a cascade:

1. **Leader active?** → peek-mode state machine handles everything; Clients see nothing.
2. **Global host chord?** (see §6.1) → host consumes.
3. **Client has focus AND does not declare `CONSUMES_ARROW_KEYS`?** → Client's `handle_key` is invoked; returns `Consumed` or `Pass`.
4. **Fall-through** → GTK widget handles normally.

Clients can't override host chords; conflict is resolved at layer 2 and the user never has to reason about it.

### 4.6 Resource handlers

```rust
pub struct ResourceHandler {
    pub resource_kind: ResourceKind,                    // File(ext?) | Directory | SshHost | Url | ...
    pub label: &'static str,
    pub open: fn(&ClientContext, &Resource, &OpenTarget) -> Result<ClientHandle>,
}
```

The default handler per resource kind is the first descriptor declaring it; overridable in settings. The "Open with…" menu always shows all registered handlers with the default highlighted.

### 4.7 Theme-token consumption

Clients read from `ThemeTokens`, not raw colors:

```rust
pub struct ThemeTokens {
    pub accent: Rgba,
    pub chrome_bg: Rgba, pub chrome_fg: Rgba,
    pub editor_bg: Rgba, pub editor_fg: Rgba,
    pub terminal_palette: Palette16,
    pub font_ui: String, pub font_mono: String,
}
```

`ClientContext` provides a live handle; tokens update on theme change and Clients re-render. New tokens are additive.

### 4.8 Unknown-kind graceful degradation

When deserializing a tab whose `kind` is unregistered, the host inserts a **Stub Client**:

- Placeholder: *"This tab was a **`<kind>`** client (not available in this install)."* + Try-again button.
- Preserves `clean_state` + `dirty_state` raw in memory so saving the workspace round-trips them losslessly.
- If the plugin is installed later, the stub upgrades in place on restart.

---

## 5. Workspace Browser

Implemented as a Client with `IS_CHROME` capability, anchored to the window's left-drawer slot. Not in `layouts.tree_json` — its state travels with the `Window` row (`drawer_width`, `drawer_visible`).

### 5.1 Drawer placement

- **Location**: left edge, via `AdwOverlaySplitView` (flyover on narrow windows, pinned on wide).
- **Toggle**: `Ctrl+B` (rebindable) + panel-menu item + a grip icon in the header.
- **Width**: user-resizable, persisted in `windows.drawer_width`.
- **Focus**: `Ctrl+0` jumps focus into the tree; `Esc` returns focus to the previously-focused Client. Arrows navigate, `Enter` opens the highlighted resource with its default handler in the active pane.

### 5.2 Tree contents (v1)

Four sections, fixed order, collapsible:

- **FILES** — the workspace root + descendants
- **LAYOUTS** — saved layouts for this workspace; ⊕ row opens template picker
- **HOSTS** — SSH bookmarks (workspace-scoped + global); ⊕ row opens host editor
- **LINKS** — entries from `resource_links` (urls / files / commands); ⊕ row opens link editor

File nodes show **●** if any open tab has a dirty sidecar referencing that file. The currently-loaded layout shows **●** next to its name.

Each section's contents come from a `ResourceSource` — a trait that produces tree nodes and watches for changes:

```rust
pub trait ResourceSource: Send + 'static {
    fn section_id(&self) -> &'static str;
    fn section_label(&self) -> &'static str;
    fn order(&self) -> i32;                             // fixed ordering; built-ins reserve 0..999
    fn list(&self, ctx: &ResourceCtx) -> Box<dyn Iterator<Item = TreeNode>>;
    fn watch(&self, ctx: &ResourceCtx, cb: Box<dyn Fn(SourceEvent)>);
}
```

Built-in sources: `FilesSource`, `LayoutsSource`, `HostsSource`, `LinksSource`. Registered `inventory`-style like Client descriptors.

### 5.3 File-tree specifics

- **Ignore rules**: `.gitignore` (via `ignore` crate) + `.gnome-deck-ignore` at the workspace root. Hidden files shown via toggle (`H` while tree focused).
- **Watcher**: `notify` crate, batched 200 ms.
- **Large-tree performance**: directories lazy-expand; never walk past a collapsed folder.

### 5.4 Open interaction

Three entry points, all producing the same `OpenWith × InTarget` matrix:

1. **Click** — single-click selects, double-click opens with default handler in active pane.
2. **Context menu** — flat list of handler+target combinations; grouped by handler, default target highlighted.
3. **Drag-and-drop** — drop on any pane → opens there with default handler. Drop on a tab-bar gap → adds a new tab in that TabHost. Drop on a pane edge → "split here" dropzone.

```rust
pub enum OpenTarget {
    ActivePane,                   // new tab in currently focused pane
    ActivePaneReuse,              // replace active tab (if same client kind + empty/clean)
    SplitRight, SplitDown, SplitLeft, SplitUp,
    NewPaneInGrid(u8, u8),
    NewWindow,                    // different workspace = different gnome-deck instance
}
```

### 5.5 Quick Open overlay

`Ctrl+P` summons a centered overlay that fuzzy-searches across every `ResourceSource`. `Enter` opens with default handler in active pane; `Ctrl+Enter` splits right; `Alt+Enter` opens picker for handler × target.

In-tree incremental filter: typing with the tree focused spawns a filter bar at the top; `Esc` clears.

**In-tree content search** (`Ctrl+Shift+F`) — ripgrep over the files section, results as a transient pseudo-section `▾ SEARCH "query"`. v1 ships as nice-to-have; first to cut on schedule pressure.

### 5.6 Workspace management from the browser

Tree header bar:

- **Workspace switcher dropdown** — recent workspaces + "Open Folder…" + "Close workspace" (returns to picker, terminates this per-workspace instance if no windows remain).
- **Refresh** — manual tree rescan.
- **Overflow menu** — rename workspace, remove from recent, edit ignore rules.

### 5.7 Failure modes

- **Root missing** — FILES section shows `"Root not found at <path>"` + Locate… / Remove actions. Other sections stay usable (SQLite-backed).
- **Permission denied** — node gets a warning icon; open returns an error toast.
- **Watcher saturation** (> 8192 files) — fallback to 5s polling on affected subtrees; toast informs user.

---

## 6. Keyboard & Peek Mode

Centralized in `gnome-deck-core::keys`. The host owns one `GtkEventControllerKey` at the `ApplicationWindow` level and dispatches per §4.5. All chords are declared in one table that the settings UI reads + edits.

### 6.1 Default chord table

**Host-level (layer 2 — always wins):**

| Chord | Action |
|---|---|
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+P` | Quick-open resource overlay |
| `Ctrl+B` | Toggle workspace drawer |
| `Ctrl+0` | Focus workspace drawer tree |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous **top-level** pane |
| `Ctrl+Alt+Tab` / `Ctrl+Alt+Shift+Tab` | Next / previous **nested** pane within focused top-level pane |
| `Ctrl+PgUp` / `Ctrl+PgDn` | Prev / next tab in focused TabHost |
| `Ctrl+1` … `Ctrl+9` | Jump to tab N in focused TabHost |
| `Ctrl+T` | New tab in focused TabHost (default client = Terminal) |
| `Ctrl+Shift+T` | Reopen last closed tab |
| `Ctrl+W` | Close focused tab (blocked if locked; prompts if dirty-editor) |
| `Ctrl+Shift+W` | Close focused pane |
| `Ctrl+\` | Split right |
| `Ctrl+-` | Split down |
| `Ctrl+Shift+G` | Convert focused pane into a 2×2 Grid |
| `Ctrl+Shift+K` | Toggle lock on focused tab |
| `F2` | Inline-rename focused tab title |
| `F3` | Open tab color picker |
| `Ctrl+S` | Save (Editor) |
| `Ctrl+Alt+S` | Save As (Editor) |
| **Leader** (default `Ctrl+Space`) | Enter peek mode |

**Client-level (layer 3, only when client widget has focus):**

| Chord | Client | Action |
|---|---|---|
| `Ctrl+Shift+C` / `Ctrl+Shift+V` | Terminal | Copy / paste |
| `Ctrl+Shift+F` | Terminal / WorkspaceBrowser | Find |
| `Ctrl+F` | Editor | Find in buffer |
| `Ctrl+H` | Editor | Find & replace |
| `Ctrl+Shift+R` | Terminal | Clear scrollback (+delete sidecar) |
| `Ctrl+Shift+M` | Editor | Cycle markdown preview placement |
| `Ctrl++` / `Ctrl+-` / `Ctrl+0` | Terminal, Editor, WebFrame | Font zoom in/out/reset |

### 6.2 Rebind & conflict resolution

- Chords persist in `global_settings` keyed `key.<command_id>`. Settings UI is generated from `CommandRegistry`. v1 rebinds via direct `sqlite3` / `gsettings` manipulation at documented paths; v2 ships a visual editor.
- On first run, core queries GNOME for colliding user-level keybindings. Known collision: **`Ctrl+Alt+Tab`** = GNOME "Switch system controls". Stash-and-rebind dance (same as `gnome-zones`):

  1. Read existing value, stash in `global_settings.stashed_ctrl_alt_tab`.
  2. Set the GNOME binding to `[]`.
  3. Our chord grabs cleanly.
  4. On uninstall / user-disable: restore the stashed value.

- User chord changes validate against all other registered commands; duplicates rejected with inline error.

### 6.3 Peek mode

State machine in `gnome-deck-core::peek`.

```
       leader press
Idle ────────────────► PeekActive(snapshot)
                          │
                          │ Arrow — move highlight across the whole tree
                          │ Ctrl+Tab / Ctrl+Shift+Tab — preview tab in
                          │     highlighted TabHost (mutates visible tab
                          │     only; originals recorded in snapshot)
                          │
                          │ Enter ─────► commit peeked_tabs,
                          │              focus = highlight; return to Idle
                          │
                          │ Esc ───────► restore every peeked tab to snapshot;
                          │              focus = original_focus; return to Idle
                          │
                          │ Any other key / focus lost ─► same as Esc
                          ▼
```

```rust
struct PeekSnapshot {
    original_focus_path: FocusPath,
    peeked_tabs: HashMap<PaneId, usize>,                // PaneId → original active tab index
}
```

Only tab-preview mutations are recorded — Arrow highlight changes don't mutate the tree and need no snapshot. Commit drops `peeked_tabs`; Esc iterates it and restores.

**Visual treatment**: window dims ~8 %; highlighted pane gets a 3 px accent-colored outline; HUD above the highlighted pane: `Pane 2 · Tab 1/4 · Enter to commit · Esc to cancel`; tab bar of the highlighted TabHost shows an "eye" glyph next to the peek index; other windows get a translucent "peek active elsewhere" scrim.

**Grid-nav algorithm**: compute every leaf pane's on-screen rectangle post-layout; spatial index; on Arrow, pick the nearest-center pane in that direction with tmux-style same-row bias for Left/Right and same-column bias for Up/Down. Simple, works at any depth.

**Peek + dirty state**: Clients are frozen during peek. Terminals still receive PTY output (VTE keeps running), no key input reaches them. Editor autosave timers pause. Esc has no side effect — scrollback / buffer state is whatever accrued from external activity.

### 6.4 Known modifier decisions

1. `Ctrl+Tab` = top-level pane cycle (not in-TabHost tab cycle). Tab cycling uses `Ctrl+PgUp/Dn` + `Ctrl+1..9`.
2. `Ctrl+Arrow` stays with the client (terminal / editor word-jump). Pane grid-nav uses peek mode or `Ctrl+Alt+Tab`.
3. `Ctrl+Alt+Tab` ↔ GNOME "Switch system controls" resolved by stash-and-rebind.
4. Leader `Ctrl+Space` conflicts with IBus input-method switching in some locales — first-run detection offers a safe fallback (`Super+Space`) if IBus is active.

---

## 7. Terminal Client

Rust type `TerminalClient`, wraps `vte::Terminal` with session-state glue. `kind_id = "terminal"`. Minimum VTE version **0.76** (modern `feed_child_binary` + scrollback APIs), which sets the `.deb` floor at Ubuntu 24.04.

### 7.1 VTE configuration

- Cursor blink off by default; configurable per-theme.
- Block / rectangular selection via Alt-drag; URL auto-select on right-click.
- Bell: visual flash (chrome border pulses accent) + optional audio via `GtkMediaFile`.
- Word-char exceptions: `-_.:/~?=&%@#+`.

### 7.2 Color-scheme pipeline

`themes.palette_json` is canonicalized to 16 ANSI + fg + bg + cursor + selection. Applying a theme is one `set_colors(fg, bg, &palette16)` + cursor/highlight calls. Per-importer code lives only in `gnome-deck-core::themes::import`.

### 7.3 Theme importers

Three loaders in `gnome-deck-core::themes::import`:

| Source | Parser | Notes |
|---|---|---|
| Base16 | `serde_yaml` → canonical | Handles classic base16 (24-color) and base24; base24 extras dropped |
| iTerm2 | `plist` → canonical | `.itermcolors` keys: Ansi 0..15, Foreground, Background, Cursor, Selection |
| Windows Terminal | `serde_json` → canonical | From `settings.json` scheme entry or a standalone JSON scheme |

User drops files into `~/.local/share/gnome-deck/themes/` (file-watched) OR imports via Settings → Themes → Import. Each imported theme is a row in `themes` with `source_kind`.

Bundled built-ins (first-run seeded, `is_builtin = 1`): Solarized Dark/Light, Gruvbox Dark/Light, Nord, Dracula, Tokyo Night/Day, Catppuccin Latte/Mocha, One Light/Dark, Ayu Light/Mirage.

### 7.4 Glyph / Nerd Font support

Monospace is a comma-separated font stack:

```
"JetBrainsMono Nerd Font, JetBrains Mono, monospace"
```

VTE ≥ 0.76 handles fallback via Pango — Nerd-Font glyphs resolve automatically even with a non-Nerd primary family. Fallback detection: if none of the configured fonts contain `U+E0A0`, a one-time toast suggests installing a Nerd Font with the relevant Flathub / `.deb` command. Per-terminal ligature toggle (default on for JetBrainsMono NF, Iosevka NF, Hack NF).

### 7.5 Tab title behavior

Three title sources, priority order:

1. **User override** (F2 / double-click) — sticky. A small lock glyph appears next to the title.
2. **PTY-announced** (OSC 0/2) — live stream.
3. **Auto from cwd** — basename of current working directory.

Clearing the override reverts to stream #2.

### 7.6 Tab color (universal, applies to any Client)

Per-tab stripe along the tab's bottom edge in the tab bar; full background tint when selected. Eight libadwaita tones plus `none`:

| Color | libadwaita token |
|---|---|
| red | `adw_red_3` |
| orange | `adw_orange_4` |
| yellow | `adw_yellow_5` |
| green | `adw_green_4` |
| teal | `adw_teal_4` |
| blue | `adw_blue_3` |
| purple | `adw_purple_3` |
| pink | `adw_pink_3` |

Tab-color is a property of the Tab, not the Client; moving a colored tab carries the color.

### 7.7 Shell integration

Snippets under `~/.local/share/gnome-deck/integration/` for bash, zsh, fish. First-run toast offers "Install shell integration for <shell>" (writes a single `source` line to the user's rc; always explicit).

The snippet:

- Emits **OSC 7** (cwd reporting) every prompt.
- Emits a gnome-deck-private OSC `\033]1337;GnomeDeckEnv;<filtered-env-JSON>\a` snapshotting the allowlist vars on prompt.
- Emits **OSC 133** prompt markers (A/B) for future "scroll to last prompt" semantics.
- Defines `gd-cd <path>` = `cd` + OSC that updates the tab title if no user override is set.

Without integration: cwd fallback via `/proc/<shell_pid>/cwd` polling; env snapshot empty; scrollback still persists; everything else works.

### 7.8 Restore semantics

`TerminalClient::new(cx, clean, dirty)`:

1. Spawn new PTY with `clean.shell` (default `$SHELL` or `/bin/bash`), in `clean.cwd`, with `clean.cols × clean.rows` initial geometry.
2. If `dirty.scrollback` present: prepend to VTE buffer as a frozen transcript region (ANSI reset wrapping, then `\r\n── restored session ──\r\n` separator).
3. If `dirty.env` present: write `export KEY=value\n` lines to PTY stdin before the first prompt — shell integration swallows them silently; raw shells execute them visibly.
4. Focus inherits from the last saved Window focus state.

### 7.9 URL detection

VTE regex detector: `http[s]://`, `file://`, `ssh://`, `mailto:`, `git://`, absolute paths. `Ctrl+Click` opens via `xdg-open` (Flatpak portal). OSC 8 hyperlinks honored with subtle underline.

### 7.10 Search

`Ctrl+Shift+F` while Terminal focused opens VTE's native regex-search bar via `vte::TerminalExtManual::search_find_previous/next`. Client-level binding (§6.1).

### 7.11 Resource handlers

Terminal registers:

- `Directory → "Open here (cd)"` — spawn in directory; if a terminal already exists in the pane, emit `cd <path>` as the first command.
- `SshHost → "Connect"` — spawn terminal, run `ssh [-i identity] [-p port] [user@]host` as initial command. Prefer `last_cwd` if SSH-side integration recorded one.

---

## 8. Editor Client

Rust type `EditorClient`. `kind_id = "editor"`. Wraps `sourceview5::View` plus an optional `webkit6::WebView` preview pane. Intentionally minimal — "a clean editor you trust, not an IDE."

### 8.1 Scope

**In:**

- Open / edit / save any UTF-8 text file; syntax highlighting for the ~100 languages GtkSourceView ships built-in.
- Dirty-buffer persistence across restart (§3.4 sidecar).
- Markdown files (`.md`, `.markdown`) get a live preview pane.
- Find / replace in file (`Ctrl+F` / `Ctrl+H`).
- External-change detection.

**Out (v2):**

- LSP, autocomplete, diagnostics.
- Multi-cursor / column-select.
- Git gutter / diff markers.
- Format-on-save, snippets.
- Non-markdown previewers (AsciiDoc, RST, Jupyter).

### 8.2 GtkSourceView configuration

- `sourceview5` with language detection on load (content-type + extension).
- Style scheme **derived from the active terminal palette** (§9.3).
- Line numbers on, tab size 4 (per language), whitespace off, wrap off by default.
- Cursor / selection / bracket-match colors all from `ThemeTokens`.

### 8.3 Markdown preview

- Parser: `pulldown-cmark` (CommonMark + tables + footnotes + strikethrough — approximately GFM).
- Renderer: `webkit6::WebView`, fed via `load_html`. CSS bundled at `~/.local/share/gnome-deck/preview.css` (user-overridable).
- **Placement**: `hidden | left | right | top | bottom`, per-tab, persisted in clean state.
- Layout mechanic: `GtkPaned` wrapping editor + preview; orientation and order derived from placement. `hidden` removes the preview from the widget tree entirely — zero WebKit overhead when off.
- **Sync scrolling**: match preview scroll to cursor via nearest-block source-map. Toggleable.
- Render debounce: 150 ms after keystroke idle. Full re-parse at typical doc sizes is cheap; no incremental parsing.
- Keybinding: `Ctrl+Shift+M` cycles placement (hidden → right → bottom → left → top → hidden). Avoids collision with the GNOME/VS Code "paste as plain text" idiom on `Ctrl+Shift+V`.

`webkit6` is a heavy dep, reused by v2 WebFrame. Feature-flagged `markdown-preview` — a no-webkit build disables both.

### 8.4 Save semantics

- `Ctrl+S`: write buffer. If disk mtime > sidecar `base_mtime`, prompt via `AdwMessageDialog`: *"File changed on disk. Overwrite / Reload / Compare?"* Compare opens a read-only split of the disk version.
- `Ctrl+Alt+S`: save-as via `GtkFileChooser`, rebinds the tab.
- On save, sidecar deletes; `base_mtime` updates.
- On close with unsaved changes: if locked, refuse close (prompt: unlock first). Else standard save / discard / cancel.
- **Autosave**: `editor.autosave_interval_sec` (default 0 = off). When on, saves every N seconds if buffer is dirty and bound. Independent of the sidecar.

### 8.5 Dirty-sidecar specifics

```json
{
  "buffer_contents": "…utf-8 text…",
  "base_mtime": 1776205697,
  "cursor": { "line": 142, "col": 18 },
  "selection": { "start": { … }, "end": { … } }
}
```

- Written on `buffer-changed`, debounced 500 ms.
- On restore: if disk mtime == `base_mtime` → load disk then replace with sidecar (normal). If disk mtime moved → load sidecar, tag tab with yellow *"⚠ out of sync with disk"* banner; Save prompts three-way Compare. If disk file is gone → load sidecar, tag *"⚠ file deleted on disk"*; Save recreates.

### 8.6 Find / replace

`Ctrl+F`: `AdwBanner`-style find bar at the bottom of the editor, case-insensitive default, regex toggle, `Enter` / `Shift+Enter` next/prev. `Ctrl+H`: replace mode in the same bar. Scope: current buffer. Workspace-wide search lives in the browser (`Ctrl+Shift+F`, §5.5).

### 8.7 Font & zoom

Editor uses `ThemeTokens::font_mono`. Zoom is per-tab, stored in sidecar only. Rebinds: `Ctrl+= / Ctrl++ / Ctrl+-` zoom; `Ctrl+0` reset.

### 8.8 Tab titles

- Auto-title: file basename. `•` prefix when dirty (complement to the `●` tab-chrome dot from §3.5).
- `F2` override: sticky user rename.

### 8.9 Resource handlers

- `File(any) → "Open"` — default for most extensions.
- `File("md"|"markdown") → "Open as Markdown"` — default for `.md`; preview defaults to `right`.
- `File("log"|"txt") → "Tail (read-only)"` — v2 nice-to-have.

### 8.10 Read-only mode

For un-writable files: no sidecar writes, Save disabled, tab gets a small lock glyph. `Ctrl+S` toasts suggesting `Ctrl+Alt+S`.

---

## 9. Theming

Two surfaces, one token pipeline. **Chrome** follows libadwaita / GTK system theme; **Clients** consume `ThemeTokens` derived from the user's selected terminal color scheme.

### 9.1 Chrome

- `AdwStyleManager` follows system light/dark.
- **Accent color** from `libadwaita ≥ 1.6` runtime API if available; else 8-color preset matching the Tab-color palette (§7.6) for visual consistency.
- Custom CSS at `~/.config/gnome-deck/style.css`, auto-loaded + hot-reloaded. Scoped with `.gnome-deck` root class.
- Zero-chrome-hardcoded-colors rule: `@accent_color`, `@window_bg_color`, `@card_shade_color`, etc. only.

### 9.2 Token pipeline

```
 user action                                                consumers
──────────                                                 ──────────
select terminal theme  ─┐
system light/dark      ─┤
accent-color override  ─┼─► ThemeResolver ──► ThemeTokens ─┬─► TerminalClient.set_colors()
font stack settings    ─┘                                   ├─► EditorClient.style_scheme
                                                            ├─► WorkspaceBrowser (status dots)
                                                            └─► future clients
```

`ThemeResolver` is a subscribable observable — Clients re-render on change, no reload.

### 9.3 Editor scheme from terminal palette

Committed mapping; overridable via `~/.local/share/gnome-deck/themes/<name>.scheme-override.json`:

```
background:       palette.bg
text:             palette.fg
comment:          ansi_8
string:           ansi_2
keyword:          ansi_5
type:             ansi_4
constant:         ansi_1 + bold
preprocessor:     ansi_3
operator:         ansi_6
error:            ansi_1 underline
current-line:     palette.bg blended 6% toward fg
selection:        palette.selection (fallback: ansi_4 at 30% alpha)
cursor:           palette.cursor
```

Produces a reasonable-if-not-artisan syntax scheme from any terminal theme. Per-workspace escape hatch: "Use system sourceview scheme instead".

### 9.4 Built-in themes

Eight families, each as a **light / dark pair** under the same name so light/dark sync doesn't lose identity:

Solarized · Gruvbox (default dark: Gruvbox Dark) · Nord · Dracula · Tokyo Night · Catppuccin · One · Ayu.

Theme settings include "sync with system light/dark" (default on).

### 9.5 Font stacks

- `font_ui` — default `"Cantarell, sans-serif"` (GNOME default).
- `font_mono` — default `"JetBrainsMono Nerd Font, JetBrains Mono, Cascadia Code, DejaVu Sans Mono, monospace"`.

Validated against Pango's font-resolution API — unresolvable families highlight red with suggestions.

### 9.6 User-authored themes

- Drop a `.itermcolors` / base16 `.yaml` / WT `.json` into `~/.local/share/gnome-deck/themes/` → auto-import on launch and directory change.
- Import dialog: Settings → Themes → Import.
- Export current theme: Settings → Themes → Export as → Base16 YAML / WT JSON / canonical `.deck-theme` JSON (round-trips losslessly).

### 9.7 Live preview

Settings → Themes shows a live preview with a mock tab bar, terminal content, and markdown-in-editor view, updating in real time. No "apply" button — selection is the action.

---

## 10. Launch & Persistence

### 10.1 Workspace resolution (first match wins)

1. No args → return Recent Workspaces picker sentinel.
2. `--new-workspace <dir>` → force-register `<dir>` as a fresh workspace.
3. `<dir>` (directory) → `workspaces.root_path = canonical(<dir>)`; insert row if missing.
4. `<file>` → walk up from `parent(file)`; nearest registered workspace owns the file. If none, register `parent(file)` as a new workspace and open the file as a tab.
5. `--workspace <id>` → open by UUID (used by the picker's Open action).

### 10.2 Recent Workspaces picker

Separate singleton GApplication (`org.gnome.Deck.picker`), shown on no-args launch, on unmatched ancestor, or via "Close workspace" from inside an instance:

```
┌─ gnome-deck ────────────────────────┐
│ Open a workspace           [+ New]  │
├─────────────────────────────────────┤
│ 📌 my-project          ~/proj/foo   │
│    gnome-power-toys    ~/dev/gpt    │
│    blog                ~/code/blog  │
├─────────────────────────────────────┤
│ 📁  Open folder…                    │
│ ✨  Start from template…            │
└─────────────────────────────────────┘
```

- 📌 = pinned (`workspaces.pinned`). Context menu: Rename, Remove from recent, Unpin/Pin, Reveal in Files.
- Typing fuzzy-filters; `Enter` opens the highlighted workspace.
- On selection the picker invokes `gnome-deck <path>` via `GSubprocess` — the new process traverses the normal launch router, which routes to an existing instance (if any) or spawns fresh. The picker stays alive across workspace launches (it's its own singleton), so a user closing a workspace returns to an already-running picker without restart cost.
- Picker window ~480 × 520 px, non-resizable, centered.

### 10.3 Named layouts within a workspace

- Every workspace has at least one layout named `default` (created on workspace-register: `Layout::Single` with one Terminal tab at workspace root).
- Window open loads `windows.active_layout_id` (fallback `default`).
- **Save current as…** via Command Palette or tree → `⊕ From current`. New row with `is_template = 0`, `workspace_id = current`.
- **Switch layouts** via palette or tree click. If current has unsaved changes since last save: *"Save changes to '<name>' before switching?"* (Save / Discard / Cancel).

**Loading a layout:**

1. Flush dirty sidecars of outgoing layout.
2. Destroy Client widgets in reverse tree order.
3. Deserialize target layout JSON.
4. Instantiate Clients via descriptors + clean state + (if present) dirty sidecars.
5. Mount; update `windows.active_layout_id`.

### 10.4 Layout templates

Shape-only starting points. `layouts` rows with `workspace_id = NULL` and `is_template = 1`.

Built-in (first-run seeded, `is_builtin = 1`):

| Name | Shape |
|---|---|
| Blank | Single pane, one Terminal tab |
| Editor + Terminal (2/3 \| 1/3) | Split H 0.66 — Editor / Terminal |
| Three-up | Split H 0.5 — left Editor (md preview right); right Split V 0.5 Editor / Terminal |
| Four-pane grid | Grid 2×2 Terminals |
| Notes | Single Editor, Markdown preview right |
| Debug session | Split H 0.5 — left Split V 0.6 Editor / Terminal; right Grid 2×2 Terminals |
| Review | Split V 0.5 — top Editor (left preview); bottom Terminal |

User-created templates: palette → "Save current layout as template" clones current tree, strips resource bindings, stores with `workspace_id = NULL, is_template = 1`.

Instantiation — from picker: pick template → pick target workspace → new named layout; from inside a workspace: tree `LAYOUTS → ⊕ From template` → pick → name → new layout in current workspace.

### 10.5 Session restore on launch

`global_settings.session_restore_on_launch` (default `true`):

- **true**: each workspace instance loads `windows.active_layout_id` with sidecars. First paint matches last-quit state.
- **false**: loads the workspace's `default`, empty sidecars.

Scope v1: per-workspace only. Multi-window restore per workspace → v2.

### 10.6 Workspace lifecycle operations

- **Create**: picker "Open folder…" or CLI `gnome-deck /path`. Seeds `default` layout.
- **Rename**: tree header overflow → Rename; updates `workspaces.name`. Root path is immutable from the app — if moved, the workspace disconnects and offers Locate… (§5.7).
- **Remove from recent**: clears `pinned`, zeros `last_opened_at`. Doesn't delete the row.
- **Delete workspace fully**: Settings → Workspaces → <name> → Delete, with confirm listing everything lost. Cascades via FK.

### 10.7 Graceful shutdown

On `gtk_main_quit`:

1. Flush every active Client's `dirty_state()` to sidecars.
2. Serialize current tree to `layouts[active_layout_id].tree_json`.
3. Update `windows.geometry_*`, `last_active_at`, `workspaces.last_opened_at`.
4. `PRAGMA optimize`, close SQLite.
5. Release D-Bus name.

On crash (SIGSEGV, OOM): sidecars from the last debounce + last-saved tree survive. Worst-case loss: up to 500 ms of editor buffer changes and up to one prompt cycle of terminal scrollback.

---

## 11. Command Palette & Quick Open

Two overlays, shared visual frame (centered, 600 × 400, `AdwDialog`). Backed by one `GtkListView` + `nucleo` fuzzy matcher.

### 11.1 Command Palette (`Ctrl+Shift+P`)

- **Source**: union of host commands + every active Client's contributions. Rescored on every keystroke with recency bias.
- **Entry shape**: `label`, optional `accelerator` (right-aligned), optional `category` prefix.
- **Actions**: `Enter` run; `Alt+Enter` pin to "Recent"; `Ctrl+Enter` open "Run on which pane?" picker for target-accepting commands.
- **Disabled commands** render dimmed at the bottom (discoverable, explains unavailability via tooltip).

### 11.2 Quick Open (`Ctrl+P`)

- **Source**: union of every `ResourceSource::list()`. New Clients' sources appear automatically.
- **Actions**: `Enter` default handler / active pane; `Ctrl+Enter` split right; `Alt+Enter` handler × target picker.

### 11.3 Input prefix grammar

| Prefix | Interpretation |
|---|---|
| `>` | Command (even inside Quick Open) |
| `@` | SSH host |
| `#` | Saved layout |
| `:` | Go-to-line in currently focused Editor tab (`:123` or `:123:col`) |
| `?` | Help — show this grammar |

No prefix = resource fuzzy search. Palette ignores all except `>`.

---

## 12. Packaging

### 12.1 Flatpak (`org.gnome.Deck`)

- Manifest: `dist/flatpak/org.gnome.Deck.yaml`.
- Runtime: `org.gnome.Platform//46`.
- SDK: `org.gnome.Sdk//46` + `org.freedesktop.Sdk.Extension.rust-stable`.
- Permissions:
  - `--filesystem=host` (unavoidable for arbitrary project roots).
  - `--socket=pulseaudio` (bell).
  - `--share=network` (SSH + future Web).
  - `--talk-name=org.freedesktop.portal.*`.
- No `--share=ipc` / `--socket=x11` — pure Wayland + session D-Bus.
- Built-in themes embedded via `gresource`; integration snippets under `/app/share/gnome-deck/integration/`.

### 12.2 Debian package (`gnome-deck`)

- Binary: `/usr/bin/gnome-deck`.
- Assets: `/usr/share/gnome-deck/{themes,integration,preview.css}`.
- Desktop entry: `/usr/share/applications/org.gnome.Deck.desktop` with `Exec=gnome-deck %U`, `MimeType=text/plain;text/markdown;inode/directory;`.
- AppStream: `/usr/share/metainfo/org.gnome.Deck.metainfo.xml`.
- No systemd unit (diverges from siblings — §1 rationale).
- Post-install: none. First launch does theme seeding, schema bootstrap, integration offer.

### 12.3 CI build matrix

- `cargo test --workspace` on Ubuntu 24.04.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo fmt --check`.
- `cargo deny check`.
- Flatpak build via `flatpak-builder --ci-build` on tag push.
- `.deb` via `cargo-deb`.
- UI smoke: headless `gtk-run` scenario opens a workspace, creates a tab, saves a layout, exits; non-zero exit on panic.

### 12.4 Versioning

SemVer. Internal crate versions independent; binary release is one `gnome-deck` version. Schema migrations (§3.7) versioned separately; code pins `LATEST_SCHEMA_VERSION`, upgrades unidirectional.

---

## 13. Testing Strategy

### 13.1 Unit tests (`gnome-deck-core`)

GTK-free, fast, every commit:

- **Tree operations** — split, grid, unsplit, move_tab, close_tab. Per op: empty-tree, max-depth, error, happy-path.
- **Serialization round-trip** — `tree → json → tree'` deterministic for every operation.
- **Peek snapshot** — for every peek sequence, final state matches expected commit / rollback.
- **Theme importers** — corpus of real-world `.itermcolors`, base16 YAML, WT JSON (vendored from upstream with attribution) round-trips canonical form to the degree each format allows.
- **SQLite migrations** — generate v1 db, apply each migration, schema-diff against target.

### 13.2 Integration tests (`gnome-deck-clients`)

`cargo test --features headless-gtk` with Weston in headless backend in CI:

- Terminal: spawn shell, feed input, assert output; restart from dirty sidecar, assert scrollback.
- Editor: load file, edit, sidecar written; restart, buffer restored; save, sidecar deleted.
- Browser: tree interactions dispatch correct handlers.

### 13.3 End-to-end tests (`gnome-deck`)

`gtk4-rs` `TestUtils` + fake-user driver:

- Launch → open folder → 3 tabs → split → save layout → quit → relaunch → assert identical state.
- Multi-instance: start workspace A, start workspace B; assert two processes, distinct D-Bus names.
- Peek choreography: leader, arrow-navigate, preview-cycle, commit / cancel, focus landing matches.

### 13.4 Manual smoke checklist

`dist/test/manual.md`:

- Theme switch mid-session propagates to open terminals and editor scheme within one redraw.
- Nerd-font-fallback toast appears on a system without a Nerd font.
- Markdown preview sync-scroll across all five placements.
- Drawer resize persists across restart.
- SSH host opens a real connection.
- v0 → v1 schema migration: no data loss.

### 13.5 Regression corpus

`tests/corpus/layouts/` holds user-contributed layout JSON blobs as serialization-compat safety net. Every schema bump runs the full corpus through `deserialize → serialize → deserialize` asserting byte-stability.

---

## 14. Out of Scope (deferred to v2)

- **Hotkey editor UI** — v1 rebinds via `sqlite3 deck.db` / `gsettings` at documented paths.
- **LSP / autocomplete / diagnostics / git gutter / format-on-save / snippets / multi-cursor** — editor stays minimal.
- **Multi-window per workspace** — schema supports it; UI path not wired in v1.
- **Web Frame client** — architectural hooks present (`webkit6` already linked for markdown preview, descriptor slot reserved); implementation is v2.
- **Log-tail client** (`.log` read-only follow mode).
- **In-tree content search** (`Ctrl+Shift+F` ripgrep) — marked cuttable in §5.5.
- **Workspace-wide find & replace.**
- **Jupyter / AsciiDoc / RST previewers.**
- **Remote workspaces over SSH** (remote file editing via local editor).
- **Layout / theme sync across machines.**
- **Published plugin ABI** — descriptor registration is stable internally; public-API guarantees wait for v2.
- **Drag-to-snap window tiling** — `gnome-zones` territory.
- **Custom bell sound per workspace.**
- **Customizable tab-color palette** (v1 is the fixed 8-tone libadwaita set).
