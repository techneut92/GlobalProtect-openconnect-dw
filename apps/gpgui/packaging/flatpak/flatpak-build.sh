#!/usr/bin/env bash
# Build (and install) the gpgui Flatpak.
#
# Prerequisites: flatpak and flatpak-builder on the host. On atomic Fedora:
#   flatpak install -y flathub org.flatpak.Builder
#   # then use:  flatpak run org.flatpak.Builder  in place of flatpak-builder
#
# Usage: apps/gpgui/packaging/flatpak/flatpak-build.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../../.." && pwd)"
manifest="$here/io.github.techneut92.gpgui.yml"
cd "$root"

builder=${FLATPAK_BUILDER:-flatpak-builder}
command -v "$builder" >/dev/null || builder="flatpak run org.flatpak.Builder"

# 1. Runtime, SDK and the matching rust-stable SDK extension.
flatpak install -y --user flathub \
  org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08 || true

# 2. Vendor the cargo registry for the sandboxed (offline) build. The generator
#    is a small upstream tool; fetch it if it isn't already here.
gen="$here/flatpak-cargo-generator.py"
[ -f "$gen" ] || curl -fsSL \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py \
  -o "$gen"
python3 "$gen" Cargo.lock -o "$here/cargo-sources.json"

# 3. Build + install to the user installation.
$builder --force-clean --user --install build-flatpak "$manifest"

echo
echo "Built. Run it with:  flatpak run io.github.techneut92.gpgui"
