#!/usr/bin/env bash
#
# Run the dcu-dbus D-Bus integration tests.
#
# These tests require a live session bus and are marked `#[ignore]` in the
# crate so that `cargo test --workspace` (which runs headless in CI) does
# not hang. They share a single session bus, and D-Bus dispatch is not
# reliable when several servers/clients are stacked in ONE process, so each
# ignored test is executed in its OWN `dbus-run-session` process (a fresh,
# isolated bus per test). This makes the suite deterministic.
#
# Usage:
#   ./scripts/run-bus-tests.sh
#
# Requires: dbus-run-session, cargo.

set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$CRATE_ROOT"

TESTS=(
  tests::dbus_server_starts
  tests::property_get_via_dbus
  tests::form_command_dispatches
  tests::scan_beacon_signal
  tests::prop_changed_signal_on_set
)

failed=0
for t in "${TESTS[@]}"; do
  echo "=== $t ==="
  if dbus-run-session -- \
      cargo test -p dcu-dbus --lib "$t" -- --ignored --test-threads=1; then
    echo "PASS $t"
  else
    echo "FAIL $t"
    failed=1
  fi
done

if [ "$failed" -ne 0 ]; then
  echo "Some bus integration tests failed."
  exit 1
fi
echo "All dcu-dbus bus integration tests passed."
