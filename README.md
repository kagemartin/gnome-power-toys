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

The `.deb` and Flatpak builds build and bundle this library automatically — no system install needed. `debian/rules` installs it into the `gnome-zones` package at `/usr/lib/gnome-power-toys/libgtk4-layer-shell.so.*` and the binaries use an `$ORIGIN`-relative RPATH to find it.

### Build the workspace

For `.deb` packaging use:

```
just build-deb         # build the .deb suite
just install-deb       # build and install locally via apt
```

For plain `cargo build` during development, point `PKG_CONFIG_PATH` at a local build of the submodule so cargo can link against it without a system install:

```
(cd third_party/gtk4-layer-shell && \
   meson setup -Dexamples=false -Ddocs=false -Dtests=false \
               -Dsmoke-tests=false -Dvapi=false -Dintrospection=false \
               --buildtype=release build && \
   meson compile -C build)
export PKG_CONFIG_PATH="$(pwd)/third_party/gtk4-layer-shell/build/meson-uninstalled:${PKG_CONFIG_PATH:-}"
export LD_LIBRARY_PATH="$(pwd)/third_party/gtk4-layer-shell/build/src:${LD_LIBRARY_PATH:-}"
cargo build --workspace
```
