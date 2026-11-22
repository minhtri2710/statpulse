#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
exec cargo test --locked --manifest-path src-tauri/Cargo.toml --lib export_bindings
