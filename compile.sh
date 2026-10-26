#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

# Locate Rust even when the file manager does not load the terminal PATH.
if ! command -v cargo >/dev/null 2>&1; then
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: Cargo not found. Install Rust to compile the project." >&2
    exit 1
fi

echo "Compiling PaintAtlas…"
cargo build --workspace "$@"
echo "Build complete. Executable: target/debug/paintatlas (or target/release/paintatlas with --release)."