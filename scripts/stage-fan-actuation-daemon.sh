#!/bin/bash
set -euo pipefail

if [ "${TAURI_ENV_PLATFORM:-}" != "darwin" ]; then
  exit 0
fi

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root/src-tauri"

profile=release
if [ "${TAURI_ENV_DEBUG:-false}" = "true" ]; then
  profile=debug
fi

target_dir=${CARGO_TARGET_DIR:-target}
case "$target_dir" in
  /*) ;;
  *) target_dir="$PWD/$target_dir" ;;
esac

build_args=(--bin fan-actuation-daemon --features daemon-bin)
if [ "$profile" = release ]; then
  build_args+=(--release)
fi

triple=${TAURI_ENV_TARGET_TRIPLE:-}
case "$triple" in
  universal-apple-darwin)
    arm_binary="$target_dir/aarch64-apple-darwin/$profile/fan-actuation-daemon"
    intel_binary="$target_dir/x86_64-apple-darwin/$profile/fan-actuation-daemon"
    cargo build "${build_args[@]}" --target aarch64-apple-darwin
    cargo build "${build_args[@]}" --target x86_64-apple-darwin
    mkdir -p "$target_dir/universal-apple-darwin/$profile"
    binary="$target_dir/universal-apple-darwin/$profile/fan-actuation-daemon"
    lipo -create "$arm_binary" "$intel_binary" -output "$binary"
    ;;
  aarch64-apple-darwin | x86_64-apple-darwin)
    binary="$target_dir/$triple/$profile/fan-actuation-daemon"
    cargo build "${build_args[@]}" --target "$triple"
    ;;
  "")
    echo "TAURI_ENV_TARGET_TRIPLE is not set; refusing to guess the host triple" >&2
    exit 1
    ;;
  *)
    echo "Unknown TAURI_ENV_TARGET_TRIPLE: $triple" >&2
    exit 1
    ;;
esac

mkdir -p bundle/macos/fan-actuation
install -m 755 "$binary" bundle/macos/fan-actuation/fan-actuation-daemon
chmod 755 bundle/macos/fan-actuation/install-fan-actuation.sh
