#!/usr/bin/env bash
# Install the Rust port as a silent drop-in replacement for the C
# wfantund / wfanctl binaries, plus everything needed to run it on real
# hardware: wpantund.conf, the systemd unit, and the D-Bus system-bus
# policy that lets the daemon own com.nestlabs.WPANTunnelDriver.
#
# The Cargo crate names are `dcutund` / `dcuctl`; production tooling and
# the TI webapp expect the upstream names `wfantund` and `wfanctl`. This
# script creates symlinks (not copies) so a single build artifact serves
# both names.
#
# Usage:
#   sudo ./install.sh [PREFIX]        # PREFIX defaults to /usr/local
#
# Environment overrides:
#   WPANTUND_SERVICE_USER   user that owns the D-Bus name   (default: root)
#   WPANTUND_SERVICE_GROUP  group allowed to call the daemon (default: root)
#   DBUS_CONFDIR            system.d policy dir (default: /etc/dbus-1/system.d)
set -euo pipefail

PREFIX="${1:-/usr/local}"
SBIN="${PREFIX}/sbin"
BIN="${PREFIX}/bin"
ETC="${PREFIX}/etc"
SYSTEMD_DIR="${PREFIX}/lib/systemd/system"
DBUS_CONFDIR="${DBUS_CONFDIR:-/etc/dbus-1/system.d}"
SERVICE_USER="${WPANTUND_SERVICE_USER:-root}"
SERVICE_GROUP="${WPANTUND_SERVICE_GROUP:-root}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# --- Locate the built binaries --------------------------------------------
# Via cargo metadata (workspace build or `cargo install`), falling back to
# binaries already on PATH.
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

# --- Configuration files ---------------------------------------------------
# Ship a ready-to-edit hardware example unconditionally. Never clobber an
# existing /etc/wpantund.conf on reinstall (it holds site-specific serial
# port / GPIO settings); seed it from the upstream example only if absent.
install -d "${ETC}"
if [[ -f "${SCRIPT_DIR}/wpantund.conf.hardware.example" ]]; then
    install -m 0644 "${SCRIPT_DIR}/wpantund.conf.hardware.example" \
        "${ETC}/wpantund.conf.example"
fi
if [[ ! -f "${ETC}/wpantund.conf" ]]; then
    if [[ -f "${REPO_ROOT}/src/wfantund/wpantund.conf" ]]; then
        install -m 0644 "${REPO_ROOT}/src/wfantund/wpantund.conf" \
            "${ETC}/wpantund.conf"
    elif [[ -f "${ETC}/wpantund.conf.example" ]]; then
        install -m 0644 "${ETC}/wpantund.conf.example" "${ETC}/wpantund.conf"
    fi
    echo "Seeded ${ETC}/wpantund.conf (edit it for your hardware)."
else
    echo "Keeping existing ${ETC}/wpantund.conf (not overwritten)."
fi

# --- D-Bus system-bus policy ----------------------------------------------
# Render the template (substitute service user/group) and install it. The
# daemon cannot own com.nestlabs.WPANTunnelDriver on the system bus without
# this file.
install -d "${DBUS_CONFDIR}"
DBUS_POLICY="${DBUS_CONFDIR}/com.nestlabs.WPANTunnelDriver.conf"
sed -e "s/@WPANTUND_SERVICE_USER@/${SERVICE_USER}/g" \
    -e "s/@WPANTUND_SERVICE_GROUP@/${SERVICE_GROUP}/g" \
    "${SCRIPT_DIR}/com.nestlabs.WPANTunnelDriver.conf.in" > "${DBUS_POLICY}"
chmod 0644 "${DBUS_POLICY}"
echo "Installed D-Bus policy ${DBUS_POLICY} (user=${SERVICE_USER}, group=${SERVICE_GROUP})."

# Pick up the new policy without a reboot.
if command -v systemctl >/dev/null 2>&1; then
    systemctl reload dbus 2>/dev/null || true
elif command -v dbus-send >/dev/null 2>&1; then
    dbus-send --system --type=method_call --dest=org.freedesktop.DBus \
        / org.freedesktop.DBus.ReloadConfig 2>/dev/null || true
fi

# --- systemd unit ----------------------------------------------------------
if command -v systemctl >/dev/null 2>&1; then
    install -d "${SYSTEMD_DIR}"
    install -m 0644 "${SCRIPT_DIR}/dcu-daemon.service" \
        "${SYSTEMD_DIR}/dcu-daemon.service"
    systemctl daemon-reload 2>/dev/null || true
    echo "Installed systemd unit dcu-daemon.service."
fi

# --- Summary ---------------------------------------------------------------
echo
echo "Installed:"
echo "  ${SBIN}/wfantund -> ${DCUTUND}"
echo "  ${BIN}/wfanctl  -> ${DCUCTL}"
echo "  ${ETC}/wpantund.conf            (daemon config — edit for your hardware)"
echo "  ${ETC}/wpantund.conf.example    (annotated TI CC13xx example)"
echo "  ${DBUS_POLICY}"
if command -v systemctl >/dev/null 2>&1; then
    echo "  ${SYSTEMD_DIR}/dcu-daemon.service"
fi
echo
echo "Next steps:"
echo "  1. Edit ${ETC}/wpantund.conf (set Config:NCP:SocketPath + reset GPIO)."
echo "  2. systemctl enable --now dcu-daemon"
echo "  3. wfanctl status        # or: dcuctl -I wfan0 status"
