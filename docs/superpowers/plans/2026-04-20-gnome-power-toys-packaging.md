# gnome-power-toys Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package the gnome-power-toys monorepo (currently: `gnome-clips`, `gnome-zones`; later: `gnome-deck`) as a **single Debian source package** producing several binary packages, and a **single Flatpak bundle** (`org.gnome.PowerToys`) containing every tool's UI and daemon.

**Tech stack:** `dpkg-buildpackage` / `debhelper`, `cargo`, `flatpak-builder`, `glib-compile-schemas`.

**Authoritative spec:** `docs/superpowers/specs/2026-04-20-gnome-power-toys-packaging.md`.

**Supersedes:** `docs/superpowers/plans/2026-04-14-gnome-clips-packaging.md` (and by extension the per-tool packaging sections in the clips/zones/deck design specs).

**Prerequisite state of the repo:**
- Workspace crates: `gnome-clips-daemon`, `gnome-clips`, `gnome-zones-daemon`, `gnome-zones`.
- Dev systemd units: `dist/systemd/gnome-clips-daemon.service`, `dist/systemd/gnome-zones-daemon.service` (both use `%h/.local/bin/...` — dev-only).
- Packaged systemd unit (already committed on this branch): `dist/systemd/gnome-clips-daemon.packaged.service` — `ExecStart=/usr/bin/gnome-clips-daemon`.
- Shell extensions: `dist/shell-extension/gnome-clips-toggle@power-toys/`, `dist/shell-extension/gnome-zones-mover@power-toys/` (each with `extension.js`, `metadata.json`, `schemas/*.gschema.xml`).

**Scope exclusions:**
- `gnome-deck` is designed but not yet implemented. This plan reserves the space for it (metapackage dependency, docs) but does not ship a `gnome-deck` binary package. That is added in a follow-up when the deck crates land.
- Actual invocations of `dpkg-buildpackage` and `flatpak-builder` require `sudo apt install` and network access; steps that need those are explicitly flagged. An agent in a restricted environment must stop at those steps and hand off to the user.

---

## File structure after this plan

```
debian/                                       # Debian source package — NOT under dist/, must be at repo root
├── control
├── compat
├── copyright
├── changelog
├── rules                                     # executable; runs cargo workspace build once, installs per-binary-package
├── source/format
├── gnome-clips.install
├── gnome-clips.postinst
├── gnome-clips.service                       # dh_installsystemduser user-scoped unit
├── gnome-zones.install
├── gnome-zones.postinst
├── gnome-zones.service                       # dh_installsystemduser user-scoped unit
├── gnome-power-toys-extensions.install
└── gnome-power-toys.install                  # metapackage — nothing to install

dist/
├── systemd/
│   ├── gnome-clips-daemon.service            # dev (already in repo)
│   ├── gnome-clips-daemon.packaged.service   # packaged, /usr/bin path (already committed)
│   ├── gnome-zones-daemon.service            # dev (already in repo)
│   └── gnome-zones-daemon.packaged.service   # NEW — packaged, /usr/bin path
├── shell-extension/                          # already in repo
│   ├── gnome-clips-toggle@power-toys/
│   └── gnome-zones-mover@power-toys/
└── flatpak/
    ├── org.gnome.PowerToys.yaml              # unified manifest
    ├── org.gnome.PowerToys.Clips.desktop
    ├── org.gnome.PowerToys.Zones.desktop
    ├── org.gnome.PowerToys.appdata.xml       # umbrella AppStream component for the Flatpak
    ├── org.gnome.Clips.service               # D-Bus activation for the clips daemon
    ├── org.gnome.Zones.service               # D-Bus activation for the zones daemon
    └── generated-sources.json                # vendored cargo sources; regenerated on dep bumps

scripts/
└── build-packages.sh                         # convenience wrapper for .deb and Flatpak builds
```

---

## Task 0: Packaged systemd unit for gnome-zones-daemon

The clips equivalent is already in the repo. Add the matching zones unit.

**Files:**
- Create: `dist/systemd/gnome-zones-daemon.packaged.service`

- [ ] **Step 1: Create the unit**

```
[Unit]
Description=gnome-zones window-zone daemon
Documentation=https://github.com/gnome-power-toys/gnome-zones
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/gnome-zones-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=graphical-session.target
```

- [ ] **Step 2: Commit**

```bash
git add dist/systemd/gnome-zones-daemon.packaged.service
git commit -m "chore(packaging): packaged systemd user unit for gnome-zones-daemon"
```

---

## Task 1: Debian source package metadata

One source package (`gnome-power-toys`), four binary packages (`gnome-clips`, `gnome-zones`, `gnome-power-toys-extensions`, `gnome-power-toys`).

**Files:**
- Create: `debian/control`
- Create: `debian/compat`
- Create: `debian/copyright`
- Create: `debian/changelog`
- Create: `debian/source/format`

Notes on dependencies:
- Both UIs use the `ksni` crate (pure-Rust StatusNotifierItem). **No** `libayatana-appindicator` dependency.
- The `gnome-clips` UI uses GTK 4 + libadwaita; same for `gnome-zones`.

- [ ] **Step 1: Create `debian/control`**

```
Source: gnome-power-toys
Section: utils
Priority: optional
Maintainer: gnome-power-toys contributors <noreply@example.com>
Build-Depends: debhelper-compat (= 13), cargo, rustc,
               libgtk-4-dev, libadwaita-1-dev,
               libssl-dev, pkg-config
Standards-Version: 4.6.0
Homepage: https://github.com/gnome-power-toys/gnome-power-toys

Package: gnome-clips
Architecture: amd64
Depends: ${shlibs:Depends}, ${misc:Depends},
         libgtk-4-1, libadwaita-1-0,
         gnome-shell, systemd,
         gnome-power-toys-extensions (>= ${source:Version})
Description: Clipboard history manager for GNOME (gnome-power-toys)
 gnome-clips provides a persistent clipboard history for GNOME desktop,
 triggered by Super+V. Supports text, images, files, HTML, and Markdown.
 Features search, pinning, tagging, and privacy controls.
 .
 Part of the gnome-power-toys suite. Install the gnome-power-toys
 metapackage to get every tool at once.

Package: gnome-zones
Architecture: amd64
Depends: ${shlibs:Depends}, ${misc:Depends},
         libgtk-4-1, libadwaita-1-0,
         gnome-shell, systemd,
         gnome-power-toys-extensions (>= ${source:Version})
Description: Window-zone layout manager for GNOME (gnome-power-toys)
 gnome-zones lets you define and snap windows to layout zones on GNOME,
 similar to Windows PowerToys FancyZones. Uses the Mutter D-Bus API with
 a Shell-extension shim fallback.
 .
 Part of the gnome-power-toys suite. Install the gnome-power-toys
 metapackage to get every tool at once.

Package: gnome-power-toys-extensions
Architecture: all
Depends: ${misc:Depends}, gnome-shell
Description: GNOME Shell extensions for the gnome-power-toys suite
 Ships the Shell extensions used by the gnome-power-toys tools:
 .
  * gnome-clips-toggle@power-toys — registers Super+V and toggles the
    gnome-clips popup via D-Bus.
  * gnome-zones-mover@power-toys — Mutter-API fallback shim for
    gnome-zones window move-resize.
 .
 Installed automatically when any gnome-power-toys tool that needs it
 is installed.

Package: gnome-power-toys
Architecture: all
Depends: ${misc:Depends},
         gnome-clips (>= ${source:Version}),
         gnome-zones (>= ${source:Version})
Description: Productivity tool suite for GNOME (metapackage)
 A collection of PowerToys-style utilities for GNOME, built from a
 single monorepo:
 .
  * gnome-clips — clipboard history manager (Super+V).
  * gnome-zones — window-zone layout manager.
 .
 This metapackage depends on every component and is the easiest way to
 install the full suite. To install a single tool, install it by name
 instead (for example, 'apt install gnome-clips').
```

- [ ] **Step 2: Create `debian/compat`**

```
13
```

- [ ] **Step 3: Create `debian/copyright`**

```
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: gnome-power-toys
Source: https://github.com/gnome-power-toys/gnome-power-toys

Files: *
Copyright: 2026 gnome-power-toys contributors
License: MIT
```

- [ ] **Step 4: Create `debian/changelog`**

```
gnome-power-toys (0.1.0-1) unstable; urgency=low

  * Initial release. Includes gnome-clips and gnome-zones.

 -- gnome-power-toys contributors <noreply@example.com>  Mon, 20 Apr 2026 00:00:00 +0000
```

- [ ] **Step 5: Create `debian/source/format`**

```
3.0 (native)
```

(Native format is intentional — this is not a Debianized upstream tarball; the monorepo itself is the source.)

- [ ] **Step 6: Commit**

```bash
git add debian/control debian/compat debian/copyright debian/changelog debian/source/format
git commit -m "chore(packaging): debian source package metadata for gnome-power-toys"
```

---

## Task 2: Debian build rules, install lists, postinst, systemd hooks

One `debian/rules` builds the whole workspace once, then lays each tool's files into its binary-package staging tree. Use dh sequencing + debhelper's `dh_installsystemduser` for user-scoped systemd units.

**Files:**
- Create: `debian/rules` (executable)
- Create: `debian/gnome-clips.install`
- Create: `debian/gnome-clips.postinst` (executable)
- Create: `debian/gnome-clips.service` (symlink to `../dist/systemd/gnome-clips-daemon.packaged.service`; consumed by `dh_installsystemduser`)
- Create: `debian/gnome-zones.install`
- Create: `debian/gnome-zones.postinst` (executable)
- Create: `debian/gnome-zones.service` (symlink to `../dist/systemd/gnome-zones-daemon.packaged.service`)
- Create: `debian/gnome-power-toys-extensions.install`
- Create: `debian/gnome-power-toys.install` (empty — metapackage)

- [ ] **Step 1: Create `debian/rules`**

```makefile
#!/usr/bin/make -f
export DH_VERBOSE = 1
export CARGO_HOME = $(CURDIR)/debian/cargo-home

# All tools share one workspace build.
%:
	dh $@ --with systemd

override_dh_auto_build:
	cargo build --release --workspace

override_dh_auto_install:
	# gnome-clips binaries
	install -Dm755 target/release/gnome-clips-daemon \
		debian/gnome-clips/usr/bin/gnome-clips-daemon
	install -Dm755 target/release/gnome-clips \
		debian/gnome-clips/usr/bin/gnome-clips

	# gnome-zones binaries
	install -Dm755 target/release/gnome-zones-daemon \
		debian/gnome-zones/usr/bin/gnome-zones-daemon
	install -Dm755 target/release/gnome-zones \
		debian/gnome-zones/usr/bin/gnome-zones

	# Shell extensions — all go in gnome-power-toys-extensions
	$(MAKE) install-extension EXT=gnome-clips-toggle@power-toys
	$(MAKE) install-extension EXT=gnome-zones-mover@power-toys

install-extension:
	install -d debian/gnome-power-toys-extensions/usr/share/gnome-shell/extensions/$(EXT)
	cp -a dist/shell-extension/$(EXT)/extension.js \
	      dist/shell-extension/$(EXT)/metadata.json \
	      $(wildcard dist/shell-extension/$(EXT)/README.md) \
	      debian/gnome-power-toys-extensions/usr/share/gnome-shell/extensions/$(EXT)/
	install -d debian/gnome-power-toys-extensions/usr/share/gnome-shell/extensions/$(EXT)/schemas
	install -Dm644 dist/shell-extension/$(EXT)/schemas/*.gschema.xml \
		debian/gnome-power-toys-extensions/usr/share/gnome-shell/extensions/$(EXT)/schemas/
	glib-compile-schemas debian/gnome-power-toys-extensions/usr/share/gnome-shell/extensions/$(EXT)/schemas/

override_dh_auto_test:
	cargo test --workspace
```

Make executable:
```bash
chmod +x debian/rules
```

- [ ] **Step 2: Create `debian/gnome-clips.install`**

Empty (the rules file places files explicitly). Create the file anyway so `debhelper` recognises the binary package; debhelper is happy with an empty install list.

```
```

- [ ] **Step 3: Create systemd-unit handoffs for `dh_installsystemduser`**

```bash
ln -sfn ../dist/systemd/gnome-clips-daemon.packaged.service debian/gnome-clips.service
ln -sfn ../dist/systemd/gnome-zones-daemon.packaged.service debian/gnome-zones.service
```

`dh_installsystemduser` picks up `debian/<pkg>.service` and installs it to `/usr/lib/systemd/user/<pkg>.service`. Because our unit file is named `gnome-clips-daemon` / `gnome-zones-daemon` but the handoff is keyed on the package name, the installed unit will be renamed to `gnome-clips.service` / `gnome-zones.service`. **Important:** rename the source units inside `dist/systemd/` to match, OR rename the handoff targets. We go with the latter — keep `gnome-*-daemon.service` as the unit's `Unit=` identity by using an explicit `--name` flag in rules:

Add to `debian/rules` at the end:

```makefile
override_dh_installsystemduser:
	dh_installsystemduser --name=gnome-clips-daemon -p gnome-clips
	dh_installsystemduser --name=gnome-zones-daemon -p gnome-zones
```

And rename the handoffs:

```bash
mv debian/gnome-clips.service debian/gnome-clips.gnome-clips-daemon.service
mv debian/gnome-zones.service debian/gnome-zones.gnome-zones-daemon.service
```

(Resolve whichever of the above two `dh_installsystemduser` invocation styles actually works on Ubuntu 22.04's debhelper; if both are fussy, fall back to manual `install -Dm644 …` in `override_dh_auto_install` targeting `debian/<pkg>/usr/lib/systemd/user/<name>.service` and skip `dh_installsystemduser` entirely. Document the decision in the commit message.)

- [ ] **Step 4: Create `debian/gnome-clips.postinst`**

```sh
#!/bin/sh
set -e

case "$1" in
    configure)
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --global enable gnome-clips-daemon.service || true
        fi
        ;;
esac

#DEBHELPER#
exit 0
```

```bash
chmod +x debian/gnome-clips.postinst
```

- [ ] **Step 5: Create `debian/gnome-zones.postinst`**

```sh
#!/bin/sh
set -e

case "$1" in
    configure)
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --global enable gnome-zones-daemon.service || true
        fi
        ;;
esac

#DEBHELPER#
exit 0
```

```bash
chmod +x debian/gnome-zones.postinst
```

- [ ] **Step 6: Create empty install lists for the remaining packages**

```bash
: > debian/gnome-zones.install
: > debian/gnome-power-toys-extensions.install
: > debian/gnome-power-toys.install
```

- [ ] **Step 7: Build the source and binary packages**

> Requires `sudo apt install debhelper devscripts build-essential` and a working Rust toolchain. If unavailable, stop and hand off to the user.

```bash
dpkg-buildpackage -us -uc -b
```

Expected outputs in the parent directory:
- `gnome-power-toys_0.1.0-1_amd64.buildinfo`
- `gnome-power-toys_0.1.0-1_amd64.changes`
- `gnome-clips_0.1.0-1_amd64.deb`
- `gnome-zones_0.1.0-1_amd64.deb`
- `gnome-power-toys-extensions_0.1.0-1_all.deb`
- `gnome-power-toys_0.1.0-1_all.deb`

- [ ] **Step 8: Verify**

```bash
sudo dpkg -i ../gnome-power-toys-extensions_*.deb \
             ../gnome-clips_*.deb \
             ../gnome-zones_*.deb \
             ../gnome-power-toys_*.deb
which gnome-clips gnome-clips-daemon gnome-zones gnome-zones-daemon
ls /usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/
ls /usr/share/gnome-shell/extensions/gnome-zones-mover@power-toys/
systemctl --global is-enabled gnome-clips-daemon.service
systemctl --global is-enabled gnome-zones-daemon.service
```

- [ ] **Step 9: Commit**

```bash
git add debian/
git commit -m "feat(packaging): debian build rules, install lists, postinsts, systemd handoffs"
```

---

## Task 3: Unified Flatpak — AppStream + desktop entries

The bundle is `org.gnome.PowerToys`, with per-tool `.desktop` files so each app appears as its own launcher.

**Files:**
- Create: `dist/flatpak/org.gnome.PowerToys.appdata.xml`
- Create: `dist/flatpak/org.gnome.PowerToys.Clips.desktop`
- Create: `dist/flatpak/org.gnome.PowerToys.Zones.desktop`

- [ ] **Step 1: Create the umbrella AppStream component**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>org.gnome.PowerToys</id>
  <name>GNOME Power Toys</name>
  <summary>A productivity tool suite for GNOME</summary>
  <description>
    <p>
      GNOME Power Toys bundles the following utilities:
    </p>
    <ul>
      <li>gnome-clips — clipboard history manager (Super+V).</li>
      <li>gnome-zones — window-zone layout manager.</li>
    </ul>
    <p>
      Each tool appears as its own launcher. The Super+V hotkey for
      gnome-clips, and the zone-editing hotkey for gnome-zones, are
      registered by companion GNOME Shell extensions. Shell extensions
      cannot live inside a Flatpak sandbox; install them with
      'apt install gnome-power-toys-extensions' on Debian/Ubuntu, or
      copy them from the 'gnome-power-toys/extensions/' resource directory
      inside this Flatpak into '~/.local/share/gnome-shell/extensions/'.
    </p>
  </description>
  <url type="homepage">https://github.com/gnome-power-toys/gnome-power-toys</url>
  <metadata_license>MIT</metadata_license>
  <project_license>MIT</project_license>
  <releases>
    <release version="0.1.0" date="2026-04-20"/>
  </releases>
  <content_rating type="oars-1.1"/>
  <provides>
    <id>org.gnome.PowerToys.Clips.desktop</id>
    <id>org.gnome.PowerToys.Zones.desktop</id>
  </provides>
</component>
```

- [ ] **Step 2: Create `org.gnome.PowerToys.Clips.desktop`**

```
[Desktop Entry]
Name=gnome-clips
Comment=Clipboard history manager
Exec=gnome-clips
Icon=org.gnome.PowerToys.Clips
Terminal=false
Type=Application
Categories=Utility;GTK;
StartupNotify=true
```

- [ ] **Step 3: Create `org.gnome.PowerToys.Zones.desktop`**

```
[Desktop Entry]
Name=gnome-zones
Comment=Window-zone layout manager
Exec=gnome-zones
Icon=org.gnome.PowerToys.Zones
Terminal=false
Type=Application
Categories=Utility;GTK;
StartupNotify=true
```

- [ ] **Step 4: Commit**

```bash
git add dist/flatpak/org.gnome.PowerToys.appdata.xml \
        dist/flatpak/org.gnome.PowerToys.Clips.desktop \
        dist/flatpak/org.gnome.PowerToys.Zones.desktop
git commit -m "chore(packaging): Flatpak AppStream and per-tool desktop entries"
```

---

## Task 4: D-Bus activation for each daemon inside the Flatpak

Inside the Flatpak sandbox there is no systemd user session to launch daemons, so each daemon is activated on demand when any UI connects to its well-known name.

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.service`
- Create: `dist/flatpak/org.gnome.Zones.service`

- [ ] **Step 1: `org.gnome.Clips.service`**

```
[D-BUS Service]
Name=org.gnome.Clips
Exec=/app/libexec/gnome-clips-daemon
```

- [ ] **Step 2: `org.gnome.Zones.service`**

```
[D-BUS Service]
Name=org.gnome.Zones
Exec=/app/libexec/gnome-zones-daemon
```

- [ ] **Step 3: Commit**

```bash
git add dist/flatpak/org.gnome.Clips.service dist/flatpak/org.gnome.Zones.service
git commit -m "feat(packaging): D-Bus activation for bundled daemons inside Flatpak"
```

---

## Task 5: Vendored cargo sources for the Flatpak build

Flatpak builds run with no network. `cargo build` must be fed a pre-resolved source set. The community tool `flatpak-cargo-generator.py` walks `Cargo.lock` and emits a `generated-sources.json` that `flatpak-builder` consumes.

**Files:**
- Create: `dist/flatpak/generated-sources.json` (committed; regenerated on dep bumps)

- [ ] **Step 1: Install generator dependencies**

> Requires sudo + network.

```bash
sudo apt install python3-aiohttp python3-tomlkit python3-yaml
```

- [ ] **Step 2: Fetch the generator**

```bash
curl -sSL -o /tmp/flatpak-cargo-generator.py \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
```

- [ ] **Step 3: Run it against the workspace Cargo.lock**

```bash
python3 /tmp/flatpak-cargo-generator.py \
  -o dist/flatpak/generated-sources.json \
  Cargo.lock
```

- [ ] **Step 4: Commit**

```bash
git add dist/flatpak/generated-sources.json
git commit -m "chore(packaging): vendor cargo sources for offline Flatpak build"
```

---

## Task 6: Flatpak manifest

One manifest produces one bundle. Both tools' binaries, daemons, `.desktop` files, D-Bus services, and the umbrella AppStream component are installed into the sandbox. The Shell extensions are shipped as a copyable resource under `/app/share/gnome-power-toys/extensions/` — Shell extensions cannot run inside a Flatpak.

**Files:**
- Create: `dist/flatpak/org.gnome.PowerToys.yaml`

- [ ] **Step 1: Manifest**

```yaml
id: org.gnome.PowerToys
runtime: org.gnome.Platform
runtime-version: '46'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable

# A multi-tool bundle has to pick *one* default for `flatpak run org.gnome.PowerToys`.
# We default to the clipboard UI; each tool also has its own .desktop launcher.
command: gnome-clips

finish-args:
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
  # Bundled daemons and UIs own their names on the session bus.
  - --own-name=org.gnome.Clips
  - --own-name=org.gnome.Clips.Ui
  - --own-name=org.gnome.Zones
  - --own-name=org.gnome.Zones.Ui
  - --filesystem=xdg-data/gnome-clips:create
  - --filesystem=xdg-data/gnome-zones:create
  # Background portal — "run while no window is open".
  - --talk-name=org.freedesktop.portal.Background
  # Mutter DisplayConfig — gnome-zones window move-resize.
  - --talk-name=org.gnome.Mutter.DisplayConfig

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/gnome-power-toys/cargo

modules:
  - name: gnome-power-toys
    buildsystem: simple
    build-commands:
      - cargo --offline build --release --workspace
      # Binaries
      - install -Dm755 target/release/gnome-clips-daemon /app/libexec/gnome-clips-daemon
      - install -Dm755 target/release/gnome-clips        /app/bin/gnome-clips
      - install -Dm755 target/release/gnome-zones-daemon /app/libexec/gnome-zones-daemon
      - install -Dm755 target/release/gnome-zones        /app/bin/gnome-zones
      # D-Bus activation
      - install -Dm644 dist/flatpak/org.gnome.Clips.service
          /app/share/dbus-1/services/org.gnome.Clips.service
      - install -Dm644 dist/flatpak/org.gnome.Zones.service
          /app/share/dbus-1/services/org.gnome.Zones.service
      # Desktop entries
      - install -Dm644 dist/flatpak/org.gnome.PowerToys.Clips.desktop
          /app/share/applications/org.gnome.PowerToys.Clips.desktop
      - install -Dm644 dist/flatpak/org.gnome.PowerToys.Zones.desktop
          /app/share/applications/org.gnome.PowerToys.Zones.desktop
      # AppStream
      - install -Dm644 dist/flatpak/org.gnome.PowerToys.appdata.xml
          /app/share/metainfo/org.gnome.PowerToys.appdata.xml
      # Shell extensions as a copyable resource (cannot run inside the Flatpak sandbox).
      - install -d /app/share/gnome-power-toys/extensions
      - cp -a dist/shell-extension/gnome-clips-toggle@power-toys
              /app/share/gnome-power-toys/extensions/
      - cp -a dist/shell-extension/gnome-zones-mover@power-toys
              /app/share/gnome-power-toys/extensions/
    sources:
      - type: dir
        path: ../..
      - generated-sources.json
```

- [ ] **Step 2: Install flatpak-builder prerequisites (once per workstation)**

> Requires sudo + network.

```bash
sudo apt install flatpak-builder
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//46 org.gnome.Sdk//46
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable
```

- [ ] **Step 3: Build the Flatpak**

```bash
flatpak-builder --force-clean build-dir dist/flatpak/org.gnome.PowerToys.yaml
```

- [ ] **Step 4: Test each tool inside the bundle**

```bash
flatpak-builder --run build-dir dist/flatpak/org.gnome.PowerToys.yaml gnome-clips
flatpak-builder --run build-dir dist/flatpak/org.gnome.PowerToys.yaml gnome-zones
```

- [ ] **Step 5: Export to local repo and install**

```bash
flatpak-builder --repo=flatpak-repo --force-clean build-dir dist/flatpak/org.gnome.PowerToys.yaml
flatpak --user remote-add --no-gpg-verify gnome-power-toys-local flatpak-repo
flatpak --user install gnome-power-toys-local org.gnome.PowerToys
flatpak run org.gnome.PowerToys               # default: gnome-clips
flatpak run --command=gnome-zones org.gnome.PowerToys
```

- [ ] **Step 6: Commit**

```bash
git add dist/flatpak/org.gnome.PowerToys.yaml
git commit -m "feat(packaging): unified org.gnome.PowerToys Flatpak manifest"
```

---

## Task 7: Build script and `.gitignore`

**Files:**
- Create: `scripts/build-packages.sh` (executable)
- Update: `.gitignore`

- [ ] **Step 1: `scripts/build-packages.sh`**

```bash
#!/usr/bin/env bash
# Builds the .deb suite and/or the unified Flatpak.
# Usage: ./scripts/build-packages.sh [--deb] [--flatpak] [--all]
set -euo pipefail

DEB=false
FLATPAK=false

for arg in "$@"; do
    case $arg in
        --deb)     DEB=true ;;
        --flatpak) FLATPAK=true ;;
        --all)     DEB=true; FLATPAK=true ;;
    esac
done

if [ "$DEB" = false ] && [ "$FLATPAK" = false ]; then
    DEB=true; FLATPAK=true
fi

echo "=== Building gnome-power-toys packages ==="

if [ "$DEB" = true ]; then
    echo "--- Building .deb suite (gnome-clips, gnome-zones, -extensions, metapackage) ---"
    dpkg-buildpackage -us -uc -b
    echo "Built:"
    ls ../gnome-clips_*.deb ../gnome-zones_*.deb \
       ../gnome-power-toys-extensions_*.deb ../gnome-power-toys_*.deb 2>/dev/null
fi

if [ "$FLATPAK" = true ]; then
    echo "--- Building unified Flatpak (org.gnome.PowerToys) ---"
    flatpak-builder --force-clean --repo=flatpak-repo \
        build-dir dist/flatpak/org.gnome.PowerToys.yaml
    echo "Built Flatpak in flatpak-repo/"
fi

echo "=== Done ==="
```

```bash
chmod +x scripts/build-packages.sh
```

- [ ] **Step 2: `.gitignore` additions**

Append to the end of the existing `.gitignore`:

```gitignore
# Packaging outputs
/build-dir/
/flatpak-repo/
/debian/.debhelper/
/debian/cargo-home/
/debian/files
/debian/*.substvars
/debian/gnome-clips/
/debian/gnome-zones/
/debian/gnome-power-toys/
/debian/gnome-power-toys-extensions/
/debian/debhelper-build-stamp
*.deb
*.ddeb
*.changes
*.buildinfo
```

- [ ] **Step 3: Commit**

```bash
git add scripts/build-packages.sh .gitignore
git commit -m "chore(packaging): build script and ignore packaging artifacts"
```

---

## Self-review checklist

- **Single source package ✓** — `debian/control` declares `Source: gnome-power-toys`. All tool binaries are subpackages.
- **Single Flatpak bundle ✓** — `dist/flatpak/org.gnome.PowerToys.yaml` is the only manifest. Per-tool `.desktop` entries provide distinct launchers; D-Bus activation starts each daemon on demand.
- **Shell extensions outside the sandbox ✓** — shipped as `gnome-power-toys-extensions` `.deb` and as a copyable resource in the Flatpak. Never bundled into the sandbox.
- **No media-keys GSettings overrides ✓** — hotkeys are owned by the Shell extensions' own schemas.
- **No phantom dependencies ✓** — `libayatana-appindicator` not in control or manifest (UIs use pure-Rust `ksni`).
- **Workspace builds once ✓** — both the `debian/rules` build and the Flatpak module run `cargo build --release --workspace` once and then split artifacts per package.
- **postinsts ✓** — each per-tool package enables its own systemd user unit via `systemctl --global enable`. `--now` is intentionally absent.
- **Metapackage ✓** — `gnome-power-toys` depends on every tool package; `apt install gnome-power-toys` gets the full suite.
- **Deck future-proofing ✓** — the metapackage, spec, and manifest all leave room to add `gnome-deck` when its crates land, without changing any other binary package.
