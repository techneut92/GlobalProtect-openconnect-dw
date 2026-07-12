#!/usr/bin/env bash
# Bump the OBS package (Ubuntu apt repo) to a new release.
#
# Replaces the manual per-release `osc` dance: point `_service` at the new
# GitHub release tarball, set the `.dsc` Version, prepend a `debian.changelog`
# entry (generated from changelog.md), and commit. OBS then rebuilds the
# Ubuntu package on its own servers and publishes it to
# download.opensuse.org/repositories/home:Techneut92:gp-client/.
#
# Usage: scripts/obs-publish.sh <version>          (e.g. 1.3.0)
#
# Requirements:
#  - `osc` configured for https://api.opensuse.org (CI writes ~/.config/osc/oscrc
#    from the OBS_USERNAME / OBS_PASSWORD secrets)
#  - the GitHub release for v<version> must already exist, with the
#    globalprotect-openconnect-dw-<version>.offline.tar.gz asset uploaded
#    (the OBS `download_url` service fetches it server-side).
set -euo pipefail

VERSION="${1:?usage: obs-publish.sh <version>}"
API=https://api.opensuse.org
PRJ=home:Techneut92:gp-client
PKG=globalprotect-openconnect-dw

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cd "$workdir"
osc -A "$API" checkout "$PRJ" "$PKG"
cd "$PRJ/$PKG"

# 1. _service → the new release tarball.
sed -i -E "s#(/releases/download/)v[0-9.]+/${PKG}-[0-9.]+(\.offline\.tar\.gz)#\1v${VERSION}/${PKG}-${VERSION}\2#" _service
grep -q "v${VERSION}/${PKG}-${VERSION}.offline.tar.gz" _service \
  || { echo "ERROR: _service was not updated to ${VERSION}"; exit 1; }

# 2. .dsc → new Debian version.
sed -i -E "s/^Version: [0-9.]+-[0-9]+$/Version: ${VERSION}-1/" "$PKG.dsc"
grep -q "^Version: ${VERSION}-1$" "$PKG.dsc" \
  || { echo "ERROR: $PKG.dsc Version was not updated to ${VERSION}-1"; exit 1; }

# 3. debian.changelog → prepend an entry generated from changelog.md's section
#    for this release (fall back to a generic line if the section is missing).
notes="$(awk -v ver="$VERSION" '
  function flush() { if (buf != "") { print buf; buf = "" } }
  $0 ~ "^## "ver" "  {grab=1; next}
  grab && /^## /     {flush(); exit}
  grab && /^- /      {flush(); buf = substr($0, 3); next}
  grab && buf != "" && /^[ ]+[^ ]/ {sub(/^[ ]+/, ""); buf = buf " " $0}
  END {flush()}
' "$repo_root/changelog.md" | sed -E 's/\*\*//g; s/[[:space:]]+/ /g')"
[ -n "$notes" ] || notes="Update to ${VERSION}. See https://github.com/techneut92/GlobalProtect-openconnect-dw/releases/tag/v${VERSION}"

{
  echo "${PKG} (${VERSION}-1) unstable; urgency=medium"
  echo
  while IFS= read -r line; do
    echo "  * ${line}"
  done <<<"$notes"
  echo
  echo " -- Dylan Westra <dylanwestra@gmail.com>  $(date -R)"
  echo
  cat debian.changelog
} > debian.changelog.new
mv debian.changelog.new debian.changelog

osc commit -m "${VERSION}: automated release bump"
echo "OBS $PRJ/$PKG bumped to ${VERSION} — build runs on the OBS servers."
