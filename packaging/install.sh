#!/usr/bin/env bash
# Install the Rust port as a silent drop-in replacement for the C
# wfantund / wfanctl binaries.
#
# The Cargo crate names are `dcutund` / `dcuctl`; production
# tooling and the TI webapp expect the upstream names `wfantund`
# and `wfanctl`. This script creates symlinks (not copies) so a
# single `cargo install` artifact serves both names.
#
# Usage:
#   sudo ./install.sh [PREFIX]   # PREFIX defaults to /usr/local
set -euo pipefail

PREFIX="${1:-/usr/local}"
SBIN="${PREFIX}/sbin"
BIN="${PREFIX}/bin"

# Locate the built binaries via cargo metadata (works for both a
# workspace build and `cargo install`). Falls back to common paths.
CARGO_ROOT="$(cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | grep -o '"target_directory":"[^"]*"' | head -1 | cut -d'"' -f4 || true)"

find_bin() {
    local name="$1"
    if [[ -n "${CARGO_ROOT}" && -x "${CARGO_ROOT}/${name}" ]]; then
        echo "${CARGO_ROOT}/${name}"
    elif command -v "${name}" >/dev/null 2>&1; then
        command -v "${name}"
    else
        echo "error: cannot find built binary '${name}' (run 'cargo build --release' first)" >&2
        return 1
    fi
}

DCUTUND="$(find_bin dcutund)"
DCUCTL="$(find_bin dcuctl)"

install -d "${SBIN}" "${BIN}"

ln -sf "${DCUTUND}" "${SBIN}/wfantund"
ln -sf "${DCUCTL}"  "${BIN}/wfanctl"

# Ship the upstream config file unchanged (the Rust daemon parses the
# same wpantund.conf key set it implements).
if [[ -f src/wfantund/wfantund.conf ]]; then
    install -d "${PREFIX}/etc"
    install -m 0644 src/wfantund/wfantund.conf "${PREFIX}/etc/wpantund.conf"
fi

# Install the systemd unit if systemd is present.
if command -v systemctl >/dev/null 2>&1; then
    install -d "${PREFIX}/lib/systemd/system"
    install -m 0644 "$(dirname "$0")/dcu-daemon.service" \
        "${PREFIX}/lib/systemd/system/dcu-daemon.service"
    echo "Installed systemd unit dcu-daemon.service (run: systemctl enable --now dcu-daemon)"
fi

echo "Installed:"
echo "  ${SBIN}/wfantund -> ${DCUTUND}"
echo "  ${BIN}/wfanctl  -> ${DCUCTL}"
