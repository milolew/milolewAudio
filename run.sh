#!/usr/bin/env bash
set -euo pipefail

source ~/.cargo/env

echo "Building milolew Audio..."
cargo build -p ma-ui --release

echo "Starting milolew Audio..."
cargo run -p ma-ui --release
