#!/usr/bin/env bash
# Checks that the app version agrees across the three places it is declared:
#   Cargo.toml                 [workspace.package] version (inherited by all crates)
#   package.json               "version"
#   src-tauri/tauri.conf.json  "version"
#
# Usage:
#   scripts/check-version.sh            # the three must agree
#   scripts/check-version.sh v0.2.0     # ...and must equal this tag minus the leading "v"
#
# Prints the versions found and exits non-zero on any mismatch.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

# First `version = "..."` line inside the [workspace.package] table.
cargo_ver="$(awk '
  /^\[/ { in_tbl = ($0 ~ /^\[workspace\.package\][[:space:]]*$/) ; next }
  in_tbl && /^[[:space:]]*version[[:space:]]*=/ {
    match($0, /"[^"]*"/); print substr($0, RSTART + 1, RLENGTH - 2); exit
  }' "$root/Cargo.toml")"

json_ver() {
  python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8")).get("version", ""))' "$1"
}
pkg_ver="$(json_ver "$root/package.json")"
tauri_ver="$(json_ver "$root/src-tauri/tauri.conf.json")"

printf '%-34s %s\n' "Cargo.toml [workspace.package]:" "${cargo_ver:-<missing>}"
printf '%-34s %s\n' "package.json:" "${pkg_ver:-<missing>}"
printf '%-34s %s\n' "src-tauri/tauri.conf.json:" "${tauri_ver:-<missing>}"

status=0
for pair in "Cargo.toml:$cargo_ver" "package.json:$pkg_ver" "src-tauri/tauri.conf.json:$tauri_ver"; do
  [ -n "${pair#*:}" ] || { echo "::error::no version found in ${pair%%:*}"; status=1; }
done
if [ "$cargo_ver" != "$pkg_ver" ] || [ "$cargo_ver" != "$tauri_ver" ]; then
  echo "::error::version mismatch: Cargo.toml=$cargo_ver package.json=$pkg_ver tauri.conf.json=$tauri_ver"
  status=1
fi

if [ $# -ge 1 ]; then
  tag="$1"
  want="${tag#v}"
  printf '%-34s %s (expects %s)\n' "tag:" "$tag" "$want"
  for pair in "Cargo.toml:$cargo_ver" "package.json:$pkg_ver" "src-tauri/tauri.conf.json:$tauri_ver"; do
    if [ "${pair#*:}" != "$want" ]; then
      echo "::error::tag $tag does not match ${pair%%:*} version ${pair#*:} (expected $want)"
      status=1
    fi
  done
fi

[ $status -eq 0 ] && echo "OK: versions agree"
exit $status
