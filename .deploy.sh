#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Git commit (if message provided, allow no-op when nothing to commit)
if [ -n "${1:-}" ]; then
    git add -A
    git commit -m "$1" || true
fi

# Git push (skip docker compose)
~/apps/.launch.sh --git-only

# Build release binary
cargo build --release -p kerai-cli

# Install binary
sudo cp tgt/release/kerai /usr/local/bin/kerai

# Restart launchd service
sudo launchctl kickstart -k system/com.primal.kerai
