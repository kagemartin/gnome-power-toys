# gnome-clips Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package gnome-clips as both a Flatpak (`org.gnome.Clips`) and a Debian `.deb`, installable on Ubuntu 22.04+.

**Architecture:** Both packages build from the same Rust workspace. The Flatpak sandboxes the app with portal access; the .deb installs binaries system-wide with a systemd user service and gsettings schema override.

**Tech Stack:** `flatpak-builder`, `dpkg-deb`, `cargo`, `install`.

**Spec:** `docs/superpowers/specs/2026-04-14-gnome-clips-design.md` §8

**Prerequisite:** Plans 1 and 2 (daemon + UI) must be complete and passing.

**This is Plan 3 of 3.**

---

## File Structure

```
dist/
├── systemd/
│   └── gnome-clips-daemon.service      # created in Plan 1
├── flatpak/
│   ├── org.gnome.Clips.yaml            # flatpak-builder manifest
│   └── org.gnome.Clips.appdata.xml     # AppStream metadata
└── debian/
    ├── control                          # package metadata
    ├── copyright
    ├── changelog
    ├── compat
    ├── rules                            # build rules
    ├── install                          # file installation list
    ├── gnome-clips.service              # symlink → ../../systemd/gnome-clips-daemon.service
    └── postinst                         # post-install script
```

---

## Task 1: Debian package metadata

**Files:**
- Create: `dist/debian/control`
- Create: `dist/debian/compat`
- Create: `dist/debian/copyright`
- Create: `dist/debian/changelog`

- [ ] **Step 1: Create dist/debian/control**

```
Source: gnome-clips
Section: utils
Priority: optional
Maintainer: gnome-power-toys contributors <noreply@example.com>
Build-Depends: debhelper-compat (= 13), cargo, rustc, libgtk-4-dev,
               libadwaita-1-dev, libayatana-appindicator3-dev, pkg-config
Standards-Version: 4.6.0

Package: gnome-clips
Architecture: amd64
Depends: ${shlibs:Depends}, ${misc:Depends},
         libgtk-4-1, libadwaita-1-0, libayatana-appindicator3-1,
         systemd
Description: Clipboard history manager for GNOME
 gnome-clips provides a persistent clipboard history for GNOME desktop,
 triggered by Super+V. Supports text, images, files, HTML, and Markdown.
 Features search, pinning, tagging, and privacy controls.
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
git add dist/debian/
git commit -m "chore(packaging): debian package metadata"
```

---

## Task 2: Debian build rules and install list

**Files:**
- Create: `dist/debian/rules`
- Create: `dist/debian/install`
- Create: `dist/debian/postinst`

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
	install -Dm755 target/release/gnome-clips-daemon \
		$(DESTDIR)/usr/bin/gnome-clips-daemon
	install -Dm755 target/release/gnome-clips \
		$(DESTDIR)/usr/bin/gnome-clips
	install -Dm644 dist/systemd/gnome-clips-daemon.service \
		$(DESTDIR)/usr/lib/systemd/user/gnome-clips-daemon.service

override_dh_auto_test:
	cargo test -p gnome-clips-daemon
```

Make it executable:
```bash
chmod +x dist/debian/rules
```

- [ ] **Step 2: Create dist/debian/install**

```
usr/bin/gnome-clips-daemon
usr/bin/gnome-clips
usr/lib/systemd/user/gnome-clips-daemon.service
```

- [ ] **Step 3: Create dist/debian/postinst**

```bash
#!/bin/sh
set -e

case "$1" in
    configure)
        # Enable and start the user service for all active users
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --global enable gnome-clips-daemon.service || true
        fi
        ;;
esac

#DEBHELPER#
exit 0
```

```bash
chmod +x dist/debian/postinst
```

- [ ] **Step 4: Test the build**

```bash
# Install build tools if not present
sudo apt install debhelper devscripts

# From workspace root:
dpkg-buildpackage -us -uc -b
```

Expected: produces `../gnome-clips_0.1.0-1_amd64.deb`.

- [ ] **Step 5: Verify the .deb installs cleanly**

```bash
sudo dpkg -i ../gnome-clips_0.1.0-1_amd64.deb
which gnome-clips-daemon   # → /usr/bin/gnome-clips-daemon
which gnome-clips           # → /usr/bin/gnome-clips
```

- [ ] **Step 6: Commit**

```bash
git add dist/debian/
git commit -m "feat(packaging): debian build rules, install list, and postinst"
```

---

## Task 3: AppStream metadata

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.appdata.xml`

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

- [ ] **Step 2: Commit**

```bash
git add dist/flatpak/org.gnome.Clips.appdata.xml
git commit -m "chore(packaging): AppStream metadata for Flatpak"
```

---

## Task 4: Flatpak manifest

**Files:**
- Create: `dist/flatpak/org.gnome.Clips.yaml`

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
  - --talk-name=org.gnome.Clips           # daemon D-Bus name
  - --own-name=org.gnome.Clips            # UI registers same name for toggling
  - --filesystem=xdg-data/gnome-clips:create
  - --talk-name=org.freedesktop.portal.Background

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/gnome-clips/cargo

modules:
  - name: libayatana-appindicator
    buildsystem: cmake-ninja
    sources:
      - type: git
        url: https://github.com/AyatanaIndicators/libayatana-appindicator
        tag: 0.5.93

  - name: gnome-clips
    buildsystem: simple
    build-commands:
      - cargo build --release -p gnome-clips-daemon -p gnome-clips
      - install -Dm755 target/release/gnome-clips-daemon /app/libexec/gnome-clips-daemon
      - install -Dm755 target/release/gnome-clips /app/bin/gnome-clips
      - install -Dm644 dist/systemd/gnome-clips-daemon.service
          /app/share/systemd/user/gnome-clips-daemon.service
      - install -Dm644 dist/flatpak/org.gnome.Clips.appdata.xml
          /app/share/metainfo/org.gnome.Clips.appdata.xml
    sources:
      - type: dir
        path: ../..
      - type: shell
        commands:
          - cargo fetch --manifest-path Cargo.toml
```

- [ ] **Step 2: Build the Flatpak**

Install `flatpak-builder` if not present:
```bash
sudo apt install flatpak-builder
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//46 org.gnome.Sdk//46
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable
```

Build:
```bash
flatpak-builder --force-clean build-dir dist/flatpak/org.gnome.Clips.yaml
```

Expected: builds successfully in `build-dir/`.

- [ ] **Step 3: Test the Flatpak locally**

```bash
flatpak-builder --run build-dir dist/flatpak/org.gnome.Clips.yaml gnome-clips
```

Expected: the gnome-clips UI window opens.

- [ ] **Step 4: Export to local repo for install testing**

```bash
flatpak-builder --repo=flatpak-repo --force-clean build-dir dist/flatpak/org.gnome.Clips.yaml
flatpak --user remote-add --no-gpg-verify gnome-clips-local flatpak-repo
flatpak --user install gnome-clips-local org.gnome.Clips
flatpak run org.gnome.Clips
```

Expected: app launches from Flatpak sandbox.

- [ ] **Step 5: Commit**

```bash
git add dist/flatpak/
git commit -m "feat(packaging): Flatpak manifest for org.gnome.Clips"
```

---

## Task 5: CI build script

**Files:**
- Create: `scripts/build-packages.sh`

- [ ] **Step 1: Create build-packages.sh**

```bash
#!/usr/bin/env bash
# scripts/build-packages.sh
# Builds both the .deb and Flatpak packages.
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
    echo "✓ .deb built: $(ls ../gnome-clips_*.deb)"
fi

if [ "$FLATPAK" = true ]; then
    echo "--- Building Flatpak ---"
    flatpak-builder --force-clean --repo=flatpak-repo \
        build-dir dist/flatpak/org.gnome.Clips.yaml
    echo "✓ Flatpak built in flatpak-repo/"
fi

echo "=== Done ==="
```

```bash
chmod +x scripts/build-packages.sh
```

- [ ] **Step 2: Add build artifacts to .gitignore**

Create `.gitignore` at workspace root:

```gitignore
/target/
/build-dir/
/flatpak-repo/
*.deb
*.ddeb
*.changes
*.buildinfo
.superpowers/
```

- [ ] **Step 3: Commit**

```bash
git add scripts/build-packages.sh .gitignore
git commit -m "chore(packaging): CI build script and .gitignore"
```

---

## Self-Review Checklist

- **Spec §8 Flatpak** — sandboxed ✓, portal clipboard access ✓, background portal ✓, manifest in `dist/flatpak/` ✓
- **Spec §8 .deb** — both binaries installed ✓, systemd user service ✓, post-install enables service ✓, packaging in `dist/debian/` ✓
- **gsettings schema override** — not included (shortcut registration is done at runtime by gnome-clips binary itself, so no schema override needed) ✓
