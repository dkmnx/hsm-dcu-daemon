#!/usr/bin/env bash
#
# property-inventory.sh — P1-7 property-surface test against a live daemon.
#
# Queries PropGet() for every known property key on the system bus and prints
# a PASS/FAIL line per key. On FAIL it prints the actual error so you can see
# exactly what went wrong on the hardware:
#
#   [PASS] NCP:Version        = "TI_WISUN_FAN/1.0"
#   [FAIL] NCP:Foo            unknown property: NCP:Foo   (daemon has no handler -> P1-7 gap)
#   [FAIL] Thread:Bar         timeout: no reply from NCP within 2000 ms
#   [FAIL] MAC:Baz            GDBus.Error: ... <spinel error>
#
# How to use it for P1-7:
#   * Run against the **Rust** daemon (dcutund/wfantund): every "unknown
#     property" FAIL is a handler the Rust port is missing.
#   * Run against the **C** daemon (reference): a PASS means the firmware
#     really exposes that key. Keys that PASS on C but FAIL(unknown) on Rust
#     are the production property set still to implement.
#   Run it once per daemon with a different --label and diff the reports.
#
# Examples:
#   # regenerate the key snapshots from the source tree (dev machine)
#   ./scripts/property-inventory.sh --extract
#
#   # test the Rust daemon on the hardware (system bus, iface wfan0)
#   sudo ./scripts/property-inventory.sh --label rust --out rust-report.txt
#
#   # test only a subset
#   ./scripts/property-inventory.sh --filter 'NCP:' --bus session
#
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# --- defaults --------------------------------------------------------------
BUS="${DCU_DBUS_BUS:-system}"
IFACE="wfan0"
DEST="com.nestlabs.WPANTunnelDriver"
IFACE_NAME="com.nestlabs.WPANTunnelDriver"
TIMEOUT_MS=2000          # D-Bus reply timeout
WRAP_TIMEOUT=5           # hard backstop (seconds) around each call
KEYS_FILE="${SCRIPT_DIR}/property-keys.txt"
HANDLED_FILE="${SCRIPT_DIR}/rust-handled-keys.txt"
OUT=""
TOOL="auto"
FILTER=""
LABEL=""
DO_EXTRACT=0
HEADER="${REPO_ROOT}/src/wfantund/wpan-properties.h"
HANDLERS_SRC="${REPO_ROOT}/crates/dcu-tunnel-daemon/src/instance/property_handlers.rs"
BASE_SRC="${REPO_ROOT}/crates/dcu-tunnel-daemon/src/instance/base.rs"
DATASET_SRC="${REPO_ROOT}/crates/dcu-tunnel-daemon/src/dataset.rs"

# --- colors (only on a terminal) -------------------------------------------
if [[ -t 1 ]]; then
    C_GREEN=$'\033[32m'; C_RED=$'\033[31m'; C_BOLD=$'\033[1m'; C_OFF=$'\033[0m'
else
    C_GREEN=""; C_RED=""; C_BOLD=""; C_OFF=""
fi

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Test every property key against a running daemon and print PASS/FAIL.

Options:
  --extract            Regenerate property-keys.txt + rust-handled-keys.txt
                       from the source tree, then exit.
  --bus NAME           D-Bus bus: system | session (default: \$DCU_DBUS_BUS or system)
  --iface NAME         Interface name in the object path (default: wfan0)
  --dest NAME          D-Bus destination (default: com.nestlabs.WPANTunnelDriver)
  --timeout MS         D-Bus reply timeout in ms (default: 2000)
  --keys FILE          Key universe file (default: scripts/property-keys.txt)
  --handled FILE       Rust-handled key file (default: scripts/rust-handled-keys.txt)
  --filter SUBSTR      Only test keys containing SUBSTR (e.g. 'NCP:', 'Thread:')
  --label NAME         Tag the report (e.g. 'rust' vs 'c') for diffing
  --out FILE           Also write the full report to FILE
  --tool NAME          Force query tool: gdbus | dbus-send (default: auto)
  --header PATH        wpan-properties.h path for --extract
  -h, --help           Show this help.
EOF
}

# --- arg parsing -----------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --extract)   DO_EXTRACT=1 ;;
        --bus)       BUS="$2"; shift ;;
        --iface)     IFACE="$2"; shift ;;
        --dest)      DEST="$2"; shift ;;
        --timeout)   TIMEOUT_MS="$2"; shift ;;
        --keys)      KEYS_FILE="$2"; shift ;;
        --handled)   HANDLED_FILE="$2"; shift ;;
        --filter)    FILTER="$2"; shift ;;
        --label)     LABEL="$2"; shift ;;
        --out)       OUT="$2"; shift ;;
        --tool)      TOOL="$2"; shift ;;
        --header)    HEADER="$2"; shift ;;
        -h|--help)   usage; exit 0 ;;
        *) echo "error: unknown option '$1'" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

OBJ_PATH="/${DEST//.//}/${IFACE}"
METHOD="${IFACE_NAME}.PropGet"

# --- extract mode: regenerate snapshots from source ------------------------
if [[ "${DO_EXTRACT}" -eq 1 ]]; then
    if [[ ! -f "${HEADER}" ]]; then
        echo "error: cannot find wpan-properties.h at ${HEADER} (use --header)" >&2
        exit 1
    fi
    # Universe: every "Xxx:Yyy" value defined in the C header.
    grep -E '#define[[:space:]]+kWPANTUNDProperty_' "${HEADER}" \
        | grep -oE '"[^"]+"' | tr -d '"' | sort -u > "${KEYS_FILE}"

    # Rust-handled set: NCP-forwarded handlers + daemon-local keys.
    {
        # 41 NCP-forwarded prop! handlers.
        grep -oE 'prop!\("[^"]+"' "${HANDLERS_SRC}" 2>/dev/null | sed -E 's/^prop!\("//; s/"$//'
        # Daemon-local keys served by handle_daemon_property_get.
        sed -n '/fn handle_daemon_property_get/,/^    }/p' "${BASE_SRC}" 2>/dev/null \
            | grep -oE '"[A-Za-z0-9]+:[A-Za-z0-9]+"' | tr -d '"'
        # StatCollector formatted properties.
        printf '%s\n' Stat:RX Stat:TX Stat:NCP Stat:Short Stat:Long
        # AddressManager / route properties served from DaemonState.
        printf '%s\n' IPv6:AllAddresses IPv6:Routes Thread:OnMeshPrefixes Thread:OffMeshRoutes NCP:State
        # Operational dataset keys.
        grep -oE '"Dataset:[A-Za-z0-9]+"' "${DATASET_SRC}" 2>/dev/null | tr -d '"'
    } | sort -u > "${HANDLED_FILE}"

    echo "Wrote $(wc -l < "${KEYS_FILE}") keys  -> ${KEYS_FILE}"
    echo "Wrote $(wc -l < "${HANDLED_FILE}") handled keys -> ${HANDLED_FILE}"
    exit 0
fi

# --- pick a query tool -----------------------------------------------------
detect_tool() {
    if [[ "${TOOL}" != "auto" ]]; then echo "${TOOL}"; return; fi
    if command -v gdbus >/dev/null 2>&1; then echo gdbus
    elif command -v dbus-send >/dev/null 2>&1; then echo dbus-send
    else echo ""; fi
}
TOOL="$(detect_tool)"
if [[ -z "${TOOL}" ]]; then
    echo "error: neither 'gdbus' nor 'dbus-send' found; install glib2/dbus tools" >&2
    exit 1
fi

# prop_get <key>  -> sets REPLY (combined output) and RC (exit code)
prop_get() {
    local key="$1"
    case "${TOOL}" in
        gdbus)
            REPLY="$(timeout "${WRAP_TIMEOUT}" gdbus call \
                --"${BUS}" --dest "${DEST}" --object-path "${OBJ_PATH}" \
                --method "${METHOD}" "${key}" 2>&1)"
            RC=$?
            ;;
        dbus-send)
            REPLY="$(timeout "${WRAP_TIMEOUT}" dbus-send \
                --"${BUS}" --print-reply --reply-timeout="${TIMEOUT_MS}" \
                --dest="${DEST}" "${OBJ_PATH}" "${METHOD}" "string:${key}" 2>&1)"
            RC=$?
            ;;
    esac
}

# extract_value <reply> <tool> -> the bare property value on stdout.
# PropGet always returns a String: gdbus prints ('value',) on one line;
# dbus-send --print-reply prints a 'method return' header then 'string "value"'.
extract_value() {
    local reply="$1" tool="$2"
    if [[ "${tool}" == "dbus-send" ]]; then
        printf '%s\n' "${reply}" \
            | grep -vE '^method return' | grep -vE '^[[:space:]]*$' | tail -1 \
            | sed -E 's/^[[:space:]]*string[[:space:]]+"//; s/"[[:space:]]*$//'
    else
        printf '%s' "${reply}" | head -1 \
            | sed -E "s/^\(?'//; s/',?\)$//; s/^\(//; s/,?\)$//"
    fi
}

# --- preflight: can we reach the daemon at all? ----------------------------
echo "${C_BOLD}Property inventory${C_OFF}  bus=${BUS} dest=${DEST} path=${OBJ_PATH} tool=${TOOL}${LABEL:+  label=${LABEL}}"
prop_get "NCP:ProtocolVersion"
if [[ ${RC} -ne 0 ]] && echo "${REPLY}" | grep -qiE 'not running|no such name|connection refused|could not connect|serviceunknown|not activatable|name has no owner'; then
    echo "${C_RED}error: daemon '${DEST}' is not on the ${BUS} bus.${C_OFF}" >&2
    echo "       Start it (systemctl start dcu-daemon) or check --bus/--dest." >&2
    echo "       Last error: ${REPLY}" >&2
    exit 1
fi

# --- load keys -------------------------------------------------------------
if [[ ! -f "${KEYS_FILE}" ]]; then
    echo "error: key file '${KEYS_FILE}' not found. Run: $(basename "$0") --extract" >&2
    exit 1
fi
mapfile -t KEYS < <(grep -vE '^[[:space:]]*(#|$)' "${KEYS_FILE}")
if [[ -n "${FILTER}" ]]; then
    mapfile -t KEYS < <(printf '%s\n' "${KEYS[@]}" | grep -F "${FILTER}")
fi
TOTAL=${#KEYS[@]}
if [[ "${TOTAL}" -eq 0 ]]; then
    echo "error: no keys to test (filter='${FILTER}')." >&2
    exit 1
fi

# handled lookup (optional)
HAVE_HANDLED=0
[[ -f "${HANDLED_FILE}" ]] && HAVE_HANDLED=1
is_handled() { [[ "${HAVE_HANDLED}" -eq 1 ]] && grep -qxF "$1" "${HANDLED_FILE}"; }

# --- run the inventory -----------------------------------------------------
PASS=0; FAIL=0
FAIL_LINES=()           # "key<TAB>reason<TAB>error"
REPORT_LINES=()

emit() {  # emit <line>  -> stdout + report buffer + optional file
    REPORT_LINES+=("$1")
    [[ -n "${OUT}" ]] && printf '%s\n' "$1" >> "${OUT}"
}
[[ -n "${OUT}" ]] && : > "${OUT}"

i=0
for key in "${KEYS[@]}"; do
    i=$((i + 1))
    prop_get "${key}"

    if [[ ${RC} -eq 0 ]]; then
        val="$(extract_value "${REPLY}" "${TOOL}" | tr '\t' ' ')"
        [[ -z "${val}" ]] && val="(empty)"
        printf '%s[PASS]%s %-32s = %s\n' "${C_GREEN}" "${C_OFF}" "${key}" "${val:0:80}"
        emit "[PASS]	${key}	${val:0:200}"
        PASS=$((PASS + 1))
    else
        # Failure: categorize the reason and surface the real error text.
        err="$(printf '%s' "${REPLY}" | tr '\n' ' ' | sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//')"
        if [[ ${RC} -ge 124 ]] || echo "${err}" | grep -qiE 'timeout|timed out'; then
            reason="timeout: no reply from NCP within ${TIMEOUT_MS} ms"
        elif echo "${err}" | grep -qi 'unknown property'; then
            reason="unknown property (daemon has no handler -> P1-7 gap)"
            is_handled "${key}" || reason="${reason} [rust: unhandled]"
        elif [[ -z "${err}" ]]; then
            reason="failed with exit code ${RC} (no error text)"
        else
            reason="${err:0:160}"
        fi
        printf '%s[FAIL]%s %-32s %s\n' "${C_RED}" "${C_OFF}" "${key}" "${reason}"
        emit "[FAIL]	${key}	${reason}"
        FAIL_LINES+=("${key}	${reason}")
        FAIL=$((FAIL + 1))
    fi
done

# --- summary ---------------------------------------------------------------
printf '\n%s==== SUMMARY%s%s ====%s\n' "${C_BOLD}" "${LABEL:+ (${LABEL})}" "" "${C_OFF}"
printf 'Total: %d   %sPASS: %d%s   %sFAIL: %d%s\n' \
    "${TOTAL}" "${C_GREEN}" "${PASS}" "${C_OFF}" "${C_RED}" "${FAIL}" "${C_OFF}"

if [[ ${FAIL} -gt 0 ]]; then
    printf '\n%sFailures:%s\n' "${C_RED}" "${C_OFF}"
    for line in "${FAIL_LINES[@]}"; do
        key="${line%%$'\t'*}"; reason="${line#*$'\t'}"
        printf '  %-32s %s\n' "${key}" "${reason}"
        emit "[FAIL-SUMMARY]	${key}	${reason}"
    done
fi

[[ -n "${OUT}" ]] && echo "Report written to ${OUT}"

# Exit non-zero if anything failed so this can gate CI / a test run.
[[ ${FAIL} -eq 0 ]]
