# gnome-clips Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package gnome-clips as both a Flatpak (`org.gnome.Clips`) and a Debian `.deb`, installable on Ubuntu 22.04+.

**Architecture:** Both packages build from the same Rust workspace and ship the same two binaries plus the `gnome-clips-toggle@power-toys` GNOME Shell extension that owns the `Super+V` accelerator.

- **.deb** — installs `/usr/bin/gnome-clips-daemon`, `/usr/bin/gnome-clips`, a systemd *user* service, and the shell extension under `/usr/share/gnome-shell/extensions/`. The service is enabled globally via `systemctl --global enable` so every user session starts it.
- **Flatpak** — bundles both binaries in the same app; D-Bus activation starts the daemon on first UI connection. The shell extension is **not** bundled (Shell extensions must live on the host); the Flatpak's AppData/README directs users to install the extension via `apt install gnome-clips-extension` or copy it from `/app/share/gnome-clips/extension/`.

**Tech Stack:** `flatpak-builder`, `dpkg-buildpackage`, `cargo`, `install`, `glib-compile-schemas`.

**Spec:** `docs/superpowers/specs/2026-04-14-gnome-clips-design.md` §8

**Spec deviations** (authoritative here):
- Spec §8 says the `.deb` "registers the default keyboard shortcut via a gsettings override in `/usr/share/glib-2.0/schemas/`". That no longer matches the code — the accelerator lives in the shell extension's own schema (`org.gnome.shell.extensions.gnome-clips-toggle`). We ship the extension instead of a media-keys override.
- Spec §8 says postinst runs `systemctl --user enable --now`. postinst runs as root and cannot reach a user session bus, so we use `systemctl --global enable` (which installs the user unit for every user's next session). `--now` is intentionally dropped; the unit will start on next login.

**Prerequisite:** Plans 1 and 2 (daemon + UI) must be complete and passing. The following must already exist: `dist/systemd/gnome-clips-daemon.service`, `dist/shell-extension/gnome-clips-toggle@power-toys/` (with `extension.js`, `metadata.json`, `schemas/*.gschema.xml`).

**This is Plan 3 of 3.**

---

## File Structure

```
dist/
├── systemd/
│   ├── gnome-clips-daemon.service            # already exists — per-user dev path
│   └── gnome-clips-daemon.packaged.service   # NEW — absolute /usr/bin path, shipped by .deb and Flatpak
├── flatpak/
│   ├── org.gnome.Clips.yaml                  # flatpak-builder manifest
│   ├── org.gnome.Clips.appdata.xml           # AppStream metadata
│   ├── org.gnome.Clips.desktop               # desktop entry for the UI
│   ├── org.gnome.Clips.service               # D-Bus session service activation for the daemon
│   └── generated-sources.json                # cargo source lock (produced by flatpak-cargo-generator)
├── shell-extension/
│   └── gnome-clips-toggle@power-toys/        # already exists
└── debian/
    ├── control
    ├── copyright
    ├── changelog
    ├── compat
    ├── rules
    ├── install
    ├── gnome-clips-daemon.user.service       # symlink → ../systemd/gnome-clips-daemon.packaged.service
    └── postinst
```

---

## Task 0: Packaged systemd unit

The existing `dist/systemd/gnome-clips-daemon.service` uses `ExecStart=%h/.local/bin/gnome-clips-daemon` — correct for `cargo install --path` local dev but wrong for a system-installed binary. Keep it for local dev; add a second unit for packaging.

**Files:**
- Create: `dist/systemd/gnome-clips-daemon.packaged.service`

- [ ] **Step 1: Create the packaged unit**

```
[Unit]
Description=gnome-clips clipboard history daemon
Documentation=https://github.com/gnome-power-toys/gnome-clips
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/gnome-clips-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=graphical-session.target
```

- [ ] **Step 2: Commit**

```bash
git add dist/systemd/gnome-clips-daemon.packaged.service
git commit -m "chore(packaging): systemd user unit with absolute binary path"
```

---

## Task 1: Debian package metadata

**Files:**
- Create: `dist/debian/control`
- Create: `dist/debian/compat`
- Create: `dist/debian/copyright`
- Create: `dist/debian/changelog`

Note on dependencies: the UI uses the `ksni` crate (pure-Rust StatusNotifierItem). There is **no** `libayatana-appindicator` dependency — do not add one.

- [ ] **Step 1: Create dist/debian/control**

```
Source: gnome-clips
Section: utils
Priority: optional
Maintainer: gnome-power-toys contributors <noreply@example.com>
Build-Depends: debhelper-compat (= 13), cargo, rustc,
               libgtk-4-dev, libadwaita-1-dev,
               libssl-dev, pkg-config
Standards-Version: 4.6.0

Package: gnome-clips
Architecture: amd64
Depends: ${shlibs:Depends}, ${misc:Depends},
         libgtk-4-1, libadwaita-1-0,
         gnome-shell, systemd,
         gnome-clips-extension (>= ${source:Version})
Description: Clipboard history manager for GNOME
 gnome-clips provides a persistent clipboard history for GNOME desktop,
 triggered by Super+V. Supports text, images, files, HTML, and Markdown.
 Features search, pinning, tagging, and privacy controls.

Package: gnome-clips-extension
Architecture: all
Depends: ${misc:Depends}, gnome-shell
Description: GNOME Shell extension that owns the Super+V hotkey for gnome-clips
 Registers the Super+V accelerator and toggles the gnome-clips popup via D-Bus.
 Installed automatically with the gnome-clips package.
```

- [ ] **Step 2: Create dist/debian/compat**

```
13
```

- [ ] **Step 3: Create dist/debian/copyright**

```
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: gnome-clips
Source: https://github.com/gnome-power-toys/gnome-clips

Files: *
Copyright: 2026 gnome-power-toys contributors
License: MIT
```

- [ ] **Step 4: Create dist/debian/changelog**

```
gnome-clips (0.1.0-1) unstable; urgency=low

  * Initial release.

 -- gnome-power-toys contributors <noreply@example.com>  Mon, 14 Apr 2026 00:00:00 +0000
```

- [ ] **Step 5: Commit**

```bash
git add dist/debian/control dist/debian/compat dist/debian/copyright dist/debian/changelog
git commit -m "chore(packaging): debian package metadata"
```

---

## Task 2: Debian build rules, install lists, and postinst

**Files:**
- Create: `dist/debian/rules`
- Create: `dist/debian/gnome-clips.install`
- Create: `dist/debian/gnome-clips-extension.install`
- Create: `dist/debian/gnome-clips.postinst`

- [ ] **Step 1: Create dist/debian/rules**

```makefile
#!/usr/bin/make -f
export DH_VERBOSE = 1
export CARGO_HOME = $(CURDIR)/debian/cargo-home

%:
	dh $@

override_dh_auto_build:
	cargo build --release -p gnome-clips-daemon -p gnome-clips

override_dh_auto_install:
	# binaries
	install -Dm755 target/release/gnome-clips-daemon \
		debian/gnome-clips/usr/bin/gnome-clips-daemon
	install -Dm755 target/release/gnome-clips \
		debian/gnome-clips/usr/bin/gnome-clips
	# systemd user unit (packaged variant with absolute path)
	install -Dm644 dist/systemd/gnome-clips-daemon.packaged.service \
		debian/gnome-clips/usr/lib/systemd/user/gnome-clips-daemon.service
	# shell extension → separate binary package
	install -d debian/gnome-clips-extension/usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys
	cp -a dist/shell-extension/gnome-clips-toggle@power-toys/extension.js \
		dist/shell-extension/gnome-clips-toggle@power-toys/metadata.json \
		dist/shell-extension/gnome-clips-toggle@power-toys/README.md \
		debian/gnome-clips-extension/usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/
	install -d debian/gnome-clips-extension/usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/schemas
	install -Dm644 dist/shell-extension/gnome-clips-toggle@power-toys/schemas/org.gnome.shell.extensions.gnome-clips-toggle.gschema.xml \
		debian/gnome-clips-extension/usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/schemas/
	# compile the extension's schema in-place (extensions load their own compiled schemas)
	glib-compile-schemas debian/gnome-clips-extension/usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/schemas/

override_dh_auto_test:
	cargo test -p gnome-clips-daemon
```

Make it executable:
```bash
chmod +x dist/debian/rules
```

- [ ] **Step 2: Create dist/debian/gnome-clips.install**

Leave empty or omit — the `override_dh_auto_install` in `rules` handles placement explicitly. (debhelper is happy as long as the files land under `debian/<pkg>/`.)

- [ ] **Step 3: Create dist/debian/gnome-clips.postinst**

```bash
#!/bin/sh
set -e

case "$1" in
    configure)
        # Enable the systemd user unit for every user's next session.
        # postinst runs as root so we cannot reach a running user bus;
        # '--global' is the correct root-level equivalent. Users will
        # get the daemon on their next login; existing sessions can run
        # 'systemctl --user start gnome-clips-daemon' manually.
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --global enable gnome-clips-daemon.service || true
        fi
        ;;
esac

#DEBHELPER#
exit 0
```

```bash
chmod +x dist/debian/gnome-clips.postinst
```

- [ ] **Step 4: Build the .deb**

> This step requires `sudo apt install debhelper devscripts` and network access. If running in a restricted environment, stop here and have a human run it.

```bash
dpkg-buildpackage -us -uc -b
```

Expected: produces `../gnome-clips_0.1.0-1_amd64.deb` and `../gnome-clips-extension_0.1.0-1_all.deb`.

- [ ] **Step 5: Verify installation**

```bash
sudo dpkg -i ../gnome-clips-extension_0.1.0-1_all.deb ../gnome-clips_0.1.0-1_amd64.deb
which gnome-clips-daemon   # → /usr/bin/gnome-clips-daemon
which gnome-clips          # → /usr/bin/gnome-clips
ls /usr/share/gnome-shell/extensions/gnome-clips-toggle@power-toys/
systemctl --global is-enabled gnome-clips-daemon.service   # → enabled
```

- [ ] **Step 6: Commit**

```bash
git add dist/debian/
git commit -m "feat(packaging): debian build rules, install lists, and postinst"
```

---

## Task 3: AppStream metadata and desktop entry

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.appdata.xml`
- Create: `dist/flatpak/org.gnome.Clips.desktop`

- [ ] **Step 1: Create appdata.xml**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>org.gnome.Clips</id>
  <name>gnome-clips</name>
  <summary>Clipboard history manager for GNOME</summary>
  <description>
    <p>
      gnome-clips provides a persistent clipboard history for GNOME desktop.
      Trigger it with Super+V to browse, search, and paste from your clipboard history.
    </p>
    <p>
      Note: the Super+V hotkey is registered by a companion GNOME Shell extension
      (gnome-clips-toggle). Shell extensions cannot live inside a Flatpak sandbox;
      install the extension from your distribution ('apt install gnome-clips-extension'
      on Debian/Ubuntu) or from extensions.gnome.org.
    </p>
    <p>Features:</p>
    <ul>
      <li>Stores text, images, files, HTML, and Markdown</li>
      <li>Search, pin, and tag clipboard entries</li>
      <li>Configurable retention (default: 7 days, 100 items)</li>
      <li>Privacy controls: app exclusion list and incognito mode</li>
    </ul>
  </description>
  <url type="homepage">https://github.com/gnome-power-toys/gnome-clips</url>
  <metadata_license>MIT</metadata_license>
  <project_license>MIT</project_license>
  <releases>
    <release version="0.1.0" date="2026-04-14"/>
  </releases>
  <content_rating type="oars-1.1"/>
</component>
```

- [ ] **Step 2: Create org.gnome.Clips.desktop**

```
[Desktop Entry]
Name=gnome-clips
Comment=Clipboard history manager
Exec=gnome-clips
Icon=org.gnome.Clips
Terminal=false
Type=Application
Categories=Utility;GTK;
StartupNotify=true
```

- [ ] **Step 3: Commit**

```bash
git add dist/flatpak/org.gnome.Clips.appdata.xml dist/flatpak/org.gnome.Clips.desktop
git commit -m "chore(packaging): AppStream metadata and desktop entry for Flatpak"
```

---

## Task 4: D-Bus activation file for the Flatpak daemon

Inside the Flatpak sandbox there is no systemd user session to run the daemon, so the UI would find nothing owning `org.gnome.Clips`. Add a session-bus activation file so the daemon is started on demand when the UI opens a proxy.

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.service`

- [ ] **Step 1: Create the service file**

```
[D-BUS Service]
Name=org.gnome.Clips
Exec=/app/libexec/gnome-clips-daemon
```

- [ ] **Step 2: Commit**

```bash
git add dist/flatpak/org.gnome.Clips.service
git commit -m "feat(packaging): D-Bus activation for bundled daemon inside Flatpak"
```

---

## Task 5: Generate offline cargo sources for Flatpak

Flatpak builds run with no network. `cargo build` must be fed a pre-resolved source set. The community tool `flatpak-cargo-generator.py` walks `Cargo.lock` and emits a `generated-sources.json` that `flatpak-builder` can consume.

**Files:**
- Create: `dist/flatpak/generated-sources.json` (committed; regenerated on dependency bumps)

- [ ] **Step 1: Fetch the generator**

> Requires network. Document it in the repo; do not re-fetch on every build.

```bash
curl -sSL -o /tmp/flatpak-cargo-generator.py \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
```

- [ ] **Step 2: Run it against the workspace Cargo.lock**

```bash
python3 /tmp/flatpak-cargo-generator.py \
  -o dist/flatpak/generated-sources.json \
  Cargo.lock
```

- [ ] **Step 3: Commit**

```bash
git add dist/flatpak/generated-sources.json
git commit -m "chore(packaging): vendor cargo sources for offline Flatpak build"
```

---

## Task 6: Flatpak manifest

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.yaml`

Note on dependencies: the UI uses the `ksni` crate, which talks StatusNotifierItem over D-Bus directly. **Do not build or link libayatana-appindicator.** The GNOME 46 Platform runtime already provides GTK 4 and libadwaita 1; no extra modules are needed.

- [ ] **Step 1: Create the manifest**

```yaml
# dist/flatpak/org.gnome.Clips.yaml
id: org.gnome.Clips
runtime: org.gnome.Platform
runtime-version: '46'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable

command: gnome-clips

finish-args:
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
  # Bundled daemon and UI own their own names on the session bus.
  - --own-name=org.gnome.Clips
  - --own-name=org.gnome.Clips.Ui
  - --filesystem=xdg-data/gnome-clips:create
  # Background portal for "run while no window is open".
  - --talk-name=org.freedesktop.portal.Background

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/gnome-clips/cargo

modules:
  - name: gnome-clips
    buildsystem: simple
    build-commands:
      - cargo --offline build --release -p gnome-clips-daemon -p gnome-clips
      - install -Dm755 target/release/gnome-clips-daemon /app/libexec/gnome-clips-daemon
      - install -Dm755 target/release/gnome-clips /app/bin/gnome-clips
      - install -Dm644 dist/flatpak/org.gnome.Clips.service
          /app/share/dbus-1/services/org.gnome.Clips.service
      - install -Dm644 dist/flatpak/org.gnome.Clips.desktop
          /app/share/applications/org.gnome.Clips.desktop
      - install -Dm644 dist/flatpak/org.gnome.Clips.appdata.xml
          /app/share/metainfo/org.gnome.Clips.appdata.xml
      # Ship the shell extension as a resource so users who want it can
      # copy it into ~/.local/share/gnome-shell/extensions/.
      - install -d /app/share/gnome-clips/extension
      - cp -a dist/shell-extension/gnome-clips-toggle@power-toys
          /app/share/gnome-clips/extension/
    sources:
      - type: dir
        path: ../..
      - generated-sources.json
```

- [ ] **Step 2: Install build prerequisites (once)**

> Requires network.

```bash
sudo apt install flatpak-builder
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//46 org.gnome.Sdk//46
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable
```

- [ ] **Step 3: Build the Flatpak**

```bash
flatpak-builder --force-clean build-dir dist/flatpak/org.gnome.Clips.yaml
```

- [ ] **Step 4: Test the Flatpak locally**

```bash
flatpak-builder --run build-dir dist/flatpak/org.gnome.Clips.yaml gnome-clips
```

- [ ] **Step 5: Export to local repo and install**

```bash
flatpak-builder --repo=flatpak-repo --force-clean build-dir dist/flatpak/org.gnome.Clips.yaml
flatpak --user remote-add --no-gpg-verify gnome-clips-local flatpak-repo
flatpak --user install gnome-clips-local org.gnome.Clips
flatpak run org.gnome.Clips
```

- [ ] **Step 6: Commit**

```bash
git add dist/flatpak/org.gnome.Clips.yaml
git commit -m "feat(packaging): Flatpak manifest with D-Bus-activated daemon"
```

---

## Task 7: CI build script and .gitignore

**Files:**
- Create: `scripts/build-packages.sh`
- Update: `.gitignore`

- [ ] **Step 1: Create build-packages.sh**

```bash
#!/usr/bin/env bash
# scripts/build-packages.sh
# Builds the .deb and/or Flatpak packages.
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

echo "=== Building gnome-clips packages ==="

if [ "$DEB" = true ]; then
    echo "--- Building .deb ---"
    dpkg-buildpackage -us -uc -b
    echo "Built: $(ls ../gnome-clips_*.deb ../gnome-clips-extension_*.deb 2>/dev/null)"
fi

if [ "$FLATPAK" = true ]; then
    echo "--- Building Flatpak ---"
    flatpak-builder --force-clean --repo=flatpak-repo \
        build-dir dist/flatpak/org.gnome.Clips.yaml
    echo "Built Flatpak in flatpak-repo/"
fi

echo "=== Done ==="
```

```bash
chmod +x scripts/build-packages.sh
```

- [ ] **Step 2: Append build-artifact entries to .gitignore**

The existing `.gitignore` already covers `target/`, `.claude/`, `.superpowers/`, etc. Append the packaging artifacts at the end:

```gitignore
# Packaging outputs
/build-dir/
/flatpak-repo/
*.deb
*.ddeb
*.changes
*.buildinfo
```

- [ ] **Step 3: Commit**

```bash
git add scripts/build-packages.sh .gitignore
git commit -m "chore(packaging): CI build script and ignore packaging artifacts"
```

---

## Self-Review Checklist

- **Spec §8 Flatpak** — sandboxed ✓; GTK/libadwaita via GNOME 46 runtime ✓; Background portal access ✓; daemon started via D-Bus activation ✓; manifest in `dist/flatpak/` ✓; shell extension shipped as a resource (not bundled into the sandbox) ✓
- **Spec §8 .deb** — both binaries installed to `/usr/bin/` ✓; systemd user unit with absolute path at `/usr/lib/systemd/user/` ✓; postinst enables the unit globally for every user's next session ✓; shell extension in its own `gnome-clips-extension` binary package that `gnome-clips` depends on ✓; packaging files in `dist/debian/` ✓
- **Shortcut registration** — owned by the `gnome-clips-toggle@power-toys` GNOME Shell extension, shipped by the `.deb` and as a copyable resource in the Flatpak. No media-keys gsettings override is written (deliberate deviation from spec §8 wording). ✓
- **No phantom dependencies** — `libayatana-appindicator` removed from both control and manifest (UI uses the pure-Rust `ksni` crate). ✓
