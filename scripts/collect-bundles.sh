#!/usr/bin/env bash
# Copies the installers produced by `tauri build` for one target triple into
# OUT_DIR under the canonical release asset names, so the published file list
# does not depend on Tauri's naming rules:
#
#   aarch64-apple-darwin      CytoFlow_<ver>_aarch64.dmg
#                             CytoFlow_<ver>_aarch64.app.tar.gz(.sig)   updater
#   x86_64-apple-darwin       CytoFlow_<ver>_x64.dmg
#                             CytoFlow_<ver>_x64.app.tar.gz(.sig)       updater
#   x86_64-pc-windows-msvc    CytoFlow_<ver>_x64_en-US.msi(.sig)
#                             CytoFlow_<ver>_x64-setup.exe(.sig)
#   x86_64-unknown-linux-gnu  CytoFlow_<ver>_amd64.deb
#                             CytoFlow_<ver>_amd64.AppImage(.sig)
#
# The .sig files are the updater signatures Tauri writes next to each
# updatable bundle when TAURI_SIGNING_PRIVATE_KEY is set
# (bundle.createUpdaterArtifacts); they must be present.
#
# Usage: scripts/collect-bundles.sh <target-triple> <version> <out-dir>
# Fails if a bundle type is missing or ambiguous (more than one match).
set -euo pipefail
[ $# -eq 3 ] || { echo "usage: $0 <target-triple> <version> <out-dir>" >&2; exit 2; }
target="$1"; ver="$2"; out="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="${CARGO_TARGET_DIR:-$root/target}/$target/release/bundle"
mkdir -p "$out"

pick() { # <subdir> <glob> <dest-name>
  shopt -s nullglob
  local matches=("$bundle/$1"/$2)
  shopt -u nullglob
  if [ ${#matches[@]} -ne 1 ]; then
    echo "::error::expected exactly one $bundle/$1/$2, found ${#matches[@]}" >&2
    ls -la "$bundle/$1" >&2 2>/dev/null || true
    exit 1
  fi
  local src="${matches[0]}"
  if [ "$(basename "$src")" != "$3" ]; then
    echo "note: renaming $(basename "$src") -> $3"
  fi
  cp "$src" "$out/$3"
  echo "collected $out/$3"
}

# <subdir> <glob> <dest-name>: the bundle plus its updater signature (<bundle>.sig).
pick_signed() {
  pick "$1" "$2" "$3"
  pick "$1" "$2.sig" "$3.sig"
}

case "$target" in
  aarch64-apple-darwin)
    pick dmg '*.dmg' "CytoFlow_${ver}_aarch64.dmg"
    pick_signed macos '*.app.tar.gz' "CytoFlow_${ver}_aarch64.app.tar.gz" ;;
  x86_64-apple-darwin)
    pick dmg '*.dmg' "CytoFlow_${ver}_x64.dmg"
    pick_signed macos '*.app.tar.gz' "CytoFlow_${ver}_x64.app.tar.gz" ;;
  x86_64-pc-windows-msvc)
    pick_signed msi '*.msi' "CytoFlow_${ver}_x64_en-US.msi"
    pick_signed nsis '*-setup.exe' "CytoFlow_${ver}_x64-setup.exe" ;;
  x86_64-unknown-linux-gnu)
    pick deb '*.deb' "CytoFlow_${ver}_amd64.deb"
    pick_signed appimage '*.AppImage' "CytoFlow_${ver}_amd64.AppImage"
    chmod +x "$out/CytoFlow_${ver}_amd64.AppImage" ;;
  *)
    echo "::error::unsupported target $target" >&2; exit 1 ;;
esac
