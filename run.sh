#!/usr/bin/env bash
set -euo pipefail

source ~/.cargo/env 2>/dev/null || true

# WSL2 graphics support
if grep -qi microsoft /proc/version 2>/dev/null; then
    if [ -d /mnt/wslg ]; then
        # WSLg (modern WSL2) — native Wayland + X11 bridge
        export DISPLAY="${DISPLAY:-:0}"
        export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
        export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/mnt/wslg/runtime-dir}"
    else
        # Legacy WSL2 — X11 forwarding via Windows host IP
        WSL_HOST=$(awk '/nameserver/ {print $2}' /etc/resolv.conf)
        export DISPLAY="${DISPLAY:-${WSL_HOST}:0}"
    fi

    # Mesa D3D12 GPU driver (WSL2 default); fall back to software rendering if unavailable
    if ! glxinfo 2>/dev/null | grep -qi "direct rendering: yes"; then
        export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"
    fi
fi

if [[ "${1:-}" == "--check" ]]; then
    echo "Running checks..."
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings
    cargo test --workspace
    echo "All checks passed."
    exit 0
fi

echo "Starting milolew Audio..."
cargo run -p ma-ui --release
