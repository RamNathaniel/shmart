#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [[ -n "${CARGO_BIN:-}" ]]; then
    cargo_bin="$CARGO_BIN"
elif command -v cargo >/dev/null 2>&1; then
    cargo_bin="$(command -v cargo)"
elif [[ -x /opt/homebrew/opt/rustup/bin/cargo ]]; then
    cargo_bin=/opt/homebrew/opt/rustup/bin/cargo
elif [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    cargo_bin="${HOME}/.cargo/bin/cargo"
else
    echo "error: Cargo was not found; install Rust or add Cargo to PATH" >&2
    exit 127
fi

export PATH="$(dirname -- "$cargo_bin"):$PATH"
cd "$project_dir"
"$cargo_bin" build --release
exec "$project_dir/target/release/shmart" "$@"
