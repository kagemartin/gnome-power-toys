#!/usr/bin/env bash
# Builds the .deb suite and/or the unified Flatpak for gnome-power-toys.
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
    # BUILD_EXTRA lets a caller pass extra flags (e.g. BUILD_EXTRA=-d to skip
    # strict Build-Depends checks on dev machines where cargo/rustc come from
    # rustup instead of apt). Leave empty for CI / Debian buildfarm builds.
    dpkg-buildpackage -us -uc -b ${BUILD_EXTRA:-}
    echo "Built:"
    ls ../gnome-clips_*.deb ../gnome-zones_*.deb \
       ../gnome-power-toys-extensions_*.deb ../gnome-power-toys_*.deb 2>/dev/null || true
fi

if [ "$FLATPAK" = true ]; then
    echo "--- Building unified Flatpak (org.gnome.PowerToys) ---"
    flatpak-builder --force-clean --repo=flatpak-repo \
        build-dir dist/flatpak/org.gnome.PowerToys.yaml
    echo "Built Flatpak in flatpak-repo/"
fi

echo "=== Done ==="
