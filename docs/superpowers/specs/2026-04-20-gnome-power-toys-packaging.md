# gnome-power-toys — Packaging

**Project:** gnome-power-toys
**Date:** 2026-04-20
**Status:** Authoritative. Supersedes the per-tool packaging sections in `gnome-clips-design.md §8`, `gnome-zones-design.md §9`, and `gnome-deck-design.md §12`.

---

## Principle

This is a monorepo. It produces **one Debian source package** (`gnome-power-toys`) and **one Flatpak bundle** (`org.gnome.PowerToys`). Individual tools (`gnome-clips`, `gnome-zones`, and later `gnome-deck`) are shipped as binary subpackages of the `.deb` and as distinct `.desktop` entries inside the Flatpak. Distributing each tool as its own source package is explicitly rejected — one repo, one unit of release.

---

## 1. Debian

### 1.1 Source package

- **Source:** `gnome-power-toys`
- `debian/` lives at the repo root (Debian convention — required for `dpkg-buildpackage`).
- `debian/rules` runs the workspace build once — `cargo build --workspace --release` — then `override_dh_auto_install` lays each tool's artifacts into the matching binary-package staging directory.

### 1.2 Binary packages

| Package                         | Arch  | Contents                                                                                                                                    | Depends on                                          |
|---------------------------------|-------|---------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------|
| `gnome-clips`                   | amd64 | `/usr/bin/gnome-clips`, `/usr/bin/gnome-clips-daemon`, systemd user unit, AppData, `.desktop`                                               | `gnome-power-toys-extensions`                       |
| `gnome-zones`                   | amd64 | `/usr/bin/gnome-zones`, `/usr/bin/gnome-zones-daemon`, systemd user unit, AppData, `.desktop`                                               | `gnome-power-toys-extensions`                       |
| `gnome-deck` (future)           | amd64 | `/usr/bin/gnome-deck`, AppData, `.desktop`. No systemd unit (see `gnome-deck-design.md §1`).                                                | —                                                   |
| `gnome-power-toys-extensions`   | all   | All GNOME Shell extensions under `/usr/share/gnome-shell/extensions/` (`gnome-clips-toggle@power-toys`, `gnome-zones-mover@power-toys`, …). | —                                                   |
| `gnome-power-toys`              | all   | Metapackage only — no files.                                                                                                                | `gnome-clips`, `gnome-zones` (and `gnome-deck` later) |

Users can `apt install gnome-clips` to pick one tool, or `apt install gnome-power-toys` for the lot.

### 1.3 Shortcut registration

Each tool's default hotkey (`Super+V` for clips, `Super+Z` for zones) is registered by that tool's Shell extension, which lives in `gnome-power-toys-extensions`. The extension owns its own GSettings schema; no `media-keys` GSettings overrides are written by any package.

### 1.4 Post-install

Each per-tool package's postinst runs `systemctl --global enable <tool>-daemon.service` (for tools that have a daemon) so the unit starts on each user's next login. `--global` is used because postinst runs as root and cannot reach a running user bus. `--now` is intentionally not used.

`gnome-power-toys-extensions` does not enable any extension automatically; the user opts in via `gnome-extensions enable` or the Extensions app.

---

## 2. Flatpak

### 2.1 One app, many launchers

- **App id:** `org.gnome.PowerToys`
- **Runtime:** `org.gnome.Platform//46`, SDK `org.gnome.Sdk//46`, plus `org.freedesktop.Sdk.Extension.rust-stable`.
- **Manifest:** `dist/flatpak/org.gnome.PowerToys.yaml`.
- **Binaries installed inside the sandbox:**
  - `/app/bin/gnome-clips`, `/app/libexec/gnome-clips-daemon`
  - `/app/bin/gnome-zones`, `/app/libexec/gnome-zones-daemon`
  - `/app/bin/gnome-deck` (when that tool lands)
- **Desktop entries**, one per tool, each with its own `Icon=` and `Exec=`:
  - `/app/share/applications/org.gnome.PowerToys.Clips.desktop` → `Exec=gnome-clips`
  - `/app/share/applications/org.gnome.PowerToys.Zones.desktop` → `Exec=gnome-zones`
  - (future) `/app/share/applications/org.gnome.PowerToys.Deck.desktop` → `Exec=gnome-deck`
- **D-Bus activation:** per-tool `.service` files under `/app/share/dbus-1/services/` map well-known names (`org.gnome.Clips`, `org.gnome.Zones`) to the corresponding daemon in `/app/libexec/`. Daemons start on demand when any UI connects.
- **Finish args** are the union of what each tool needs (Wayland socket, fallback X11, DRI, clipboard portal for clips, background portal, bus names owned by the tools).

### 2.2 Shell extensions are not bundled

GNOME Shell extensions cannot live inside a Flatpak sandbox. The Flatpak ships each extension as a copyable resource under `/app/share/gnome-power-toys/extensions/<extension-id>/`. AppData directs users to install the host `gnome-power-toys-extensions` `.deb` (on Debian/Ubuntu) or copy the extension from the resource directory into `~/.local/share/gnome-shell/extensions/`.

---

## 3. CI

One pipeline per push:

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `dpkg-buildpackage -us -uc -b` → all binary `.deb`s (tagged releases only).
5. `flatpak-builder --repo=… dist/flatpak/org.gnome.PowerToys.yaml` (tagged releases only).

---

## 4. Supersessions of prior spec text

- `gnome-clips-design.md §8` — packaged as `org.gnome.Clips` Flatpak and a `gnome-clips` source package. **Superseded.** Clips ships as the `gnome-clips` binary package inside the `gnome-power-toys` source package, and as `org.gnome.PowerToys.Clips.desktop` inside the unified Flatpak.
- `gnome-zones-design.md §9` — packaged as `org.gnome.Zones` Flatpak and its own `.deb`. **Superseded.** Zones ships as the `gnome-zones` binary package inside the `gnome-power-toys` source package, and as `org.gnome.PowerToys.Zones.desktop` inside the unified Flatpak.
- `gnome-deck-design.md §12` — packaged as `org.gnome.Deck` Flatpak and a `gnome-deck` `.deb`. **Superseded** on the same grounds.

Each tool's design spec defers to this document for all packaging questions.
