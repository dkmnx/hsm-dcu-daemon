#!/usr/bin/env bash
# Run D-Bus integration tests that require a session bus.
#
# Usage: ./scripts/run-dbus-tests.sh
#
# These tests are #[ignore] in normal `cargo test` because they need
# a live D-Bus session bus. This script runs them under dbus-run-session.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Running D-Bus integration tests under dbus-run-session..."

if command -v dbus-run-session &>/dev/null; then
    dbus-run-session -- cargo test --workspace -- --ignored --test-threads=1
else
    echo "ERROR: dbus-run-session not found. Install dbus-x11 or dbus-daemon."
    echo "On Debian/Ubuntu: sudo apt install dbus-x11"
    echo "On Fedora: sudo dnf install dbus-x11"
    exit 1
fi
