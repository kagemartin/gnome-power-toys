# gnome-power-toys — development recipes.
#
# Install `just`: https://github.com/casey/just
# On Ubuntu/Debian: `sudo apt install just` (or `cargo install just`).

set shell := ["bash", "-euo", "pipefail", "-c"]

# Show the list of recipes.
default:
    @just --list

# Requires: cargo, dch (from devscripts), sed, date.
#
# Bump Cargo.toml, Cargo.lock, debian/changelog, AppStream release list.
bump-version VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ ! "{{VERSION}}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "VERSION must be X.Y.Z (got: {{VERSION}})" >&2
        exit 1
    fi
    # Workspace crate versions — scoped to each crate's [package] section so
    # we don't touch `version = "..."` lines under [dependencies.*] tables.
    for c in crates/*/Cargo.toml; do
        sed -i -E '/^\[package\]/,/^\[/ s/^version = ".*"/version = "{{VERSION}}"/' "$c"
    done
    cargo update --workspace --quiet
    dch --newversion "{{VERSION}}-1" --distribution unstable \
        "Release {{VERSION}}."
    TODAY=$(date --iso-8601)
    sed -i -E "/<releases>/a\\    <release version=\"{{VERSION}}\" date=\"$TODAY\"/>" \
        dist/flatpak/org.gnome.PowerToys.appdata.xml
    echo "Bumped to {{VERSION}}. Review the diff, then commit."

# Requires: python3-aiohttp python3-tomlkit python3-yaml. Network access.
#
# Regenerate dist/flatpak/generated-sources.json from Cargo.lock.
regen-flatpak-sources:
    #!/usr/bin/env bash
    set -euo pipefail
    curl -sSL --fail --max-time 60 \
        -o /tmp/flatpak-cargo-generator.py \
        https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
    python3 /tmp/flatpak-cargo-generator.py \
        -o dist/flatpak/generated-sources.json \
        Cargo.lock
    echo "Regenerated dist/flatpak/generated-sources.json."

# Build the .deb suite via dpkg-buildpackage (passes -d for local dev).
build-deb:
    BUILD_EXTRA=-d ./scripts/build-packages.sh --deb

# Build the unified org.gnome.PowerToys Flatpak.
build-flatpak:
    ./scripts/build-packages.sh --flatpak

# Build both the .deb suite and the Flatpak.
build-all:
    BUILD_EXTRA=-d ./scripts/build-packages.sh --all
