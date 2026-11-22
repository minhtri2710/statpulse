#!/bin/bash
# StatPulse macOS DMG Release Build Script
#
# Produces exactly one canonical artifact: the bundler's official
# ${product}_${version}_${arch}.dmg for the native host architecture, copied
# read-only into literal repo-local releases/.

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

version=$(node -p "require('./package.json').version")
product=StatPulse

# Cargo interprets a relative CARGO_TARGET_DIR from the crate directory, so
# resolve it to one absolute repo-local path before invoking npm.
case "${CARGO_TARGET_DIR:-}" in
  "") target_dir="$repo_root/src-tauri/target" ;;
  /*) target_dir="$CARGO_TARGET_DIR" ;;
  *) target_dir="$repo_root/$CARGO_TARGET_DIR" ;;
esac
export CARGO_TARGET_DIR="$target_dir"

echo "🦀 Compiling Rust Backend & Packaging DMG..."
npm run tauri build

host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
  x86_64-apple-darwin) arch=x64 ;;
  aarch64-apple-darwin) arch=aarch64 ;;
  *) echo "Unsupported build host: $host" >&2; exit 1 ;;
esac
bundle="$target_dir/release/bundle"
app="$bundle/macos/$product.app"
dmg="$bundle/dmg/${product}_${version}_${arch}.dmg"

if [ ! -d "$app/Contents" ]; then
  echo "Missing app bundle: $app" >&2
  exit 1
fi
scripts/verify-fan-actuation-bundle.sh "$app"

if [ ! -f "$dmg" ] || [ -L "$dmg" ] || [ ! -s "$dmg" ]; then
  echo "Missing or invalid DMG: $dmg" >&2
  exit 1
fi

# releases/ must be a literal repo directory; existing destinations are only
# acceptable when identical regular files, never overwritten.
if [ -L "$repo_root/releases" ] || { [ -e "$repo_root/releases" ] && [ ! -d "$repo_root/releases" ]; }; then
  echo "Invalid releases directory: $repo_root/releases" >&2
  exit 1
fi
mkdir -p "$repo_root/releases"
dest="$repo_root/releases/$(basename -- "$dmg")"
if [ -e "$dest" ] || [ -L "$dest" ]; then
  if [ ! -f "$dest" ] || [ -L "$dest" ] || ! cmp -s "$dmg" "$dest"; then
    echo "Refusing to overwrite existing file: $dest" >&2
    exit 1
  fi
else
  cp -n -- "$dmg" "$dest"
fi
cmp -s "$dmg" "$dest" || { echo "Copied DMG differs: $dest" >&2; exit 1; }

echo ""
echo "✅ StatPulse macOS DMG build complete!"
echo "   Location: $dest"
