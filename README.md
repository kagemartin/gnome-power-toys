# gnome-power-toys

A collection of GNOME desktop utilities.

## Crates

- **`gnome-zones-daemon`** — background service that manages tiling-zone layouts, window snapping, and hotkeys. Exposes `org.gnome.Zones` on the session D-Bus.
- **`gnome-zones`** — GTK4/libadwaita UI for the daemon: zone editor overlay, activator overlay, and panel tray icon.

## Building

### System dependencies (Ubuntu 24.04 / Noble)

```
sudo apt install \
  libgtk-4-dev libadwaita-1-dev libgdk-pixbuf-2.0-dev \
  libwayland-dev gobject-introspection libgirepository1.0-dev \
  meson ninja-build
```

### Vendored dependency: `gtk4-layer-shell`

Ubuntu Noble ships only the GTK3 version (`libgtk-layer-shell-dev`). The GTK4 variant is vendored as a git submodule at `third_party/gtk4-layer-shell` (pinned to upstream tag `v1.3.0`).

Clone with submodules:

```
git clone --recurse-submodules <repo-url>
# or, after an ordinary clone:
git submodule update --init --recursive
```

Build and install the library system-wide (required for the `gnome-zones` crate to link):

```
cd third_party/gtk4-layer-shell
meson setup -Dexamples=false -Ddocs=false -Dtests=false -Dvapi=false --buildtype=release build
meson compile -C build
sudo meson install -C build
sudo ldconfig
```

### Build the workspace

```
cargo build --workspace
```
