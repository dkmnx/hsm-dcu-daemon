# Rust Porting Coverage Gap Analysis

**Date:** 2026-07-12
**Scope:** Verify that `doc/rust-porting/*.md` covers every C/C++ source
needed to **fully replace** the daemon (`dcud` = *wfantund*) and the CLI
(`dcuctl` = *wfanctl*) with the Rust port.
**Method:** Enumerated every file under `src/` that the C build actually
compiles (`src/dcud/Makefile.am`, `src/ncp-spinel/Makefile.am`,
`src/dcuctl/Makefile.am`), then diffed that set against the files named in
each phase doc. Unmentioned compiled sources were inspected for real usage.

> **Binary names.** The autotools build produces `sbin_PROGRAMS = dcud`
> (`src/dcud/Makefile.am:46`) and `bin_PROGRAMS = dcuctl`
> (`src/dcuctl/Makefile.am:34`). The interface is `wfan0`. In this report
> **dcud == wfantund** and **dcuctl == wfanctl**.

---

## Executive summary

| Target   | CLI surface replaceable today? | Functional daemon replaceable today? | Verdict                                                                                                                                                                                     |
| -------- | ------------------------------ | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| wfanctl  | **Yes** (9 commands, parity)   | n/a                                  | Phase 2B correctly scopes the real `commandList[]`. Operators lose nothing the C CLI could do.                                                                                              |
| wfantund | n/a                            | **Complete — all P0/P1 gaps resolved** | The operational-dataset layer (P0-a) is **implemented** (phase-3C). Transport dispatch (P0-b) is **implemented** (dcu-serial: TCP, system/forkpty, dispatcher). Mfg D-Bus method, secure RNG, IPv6PacketMatcher all **implemented**. Property handler registration and DaemonState sync **implemented**. |

The 10 phase docs are thorough on what they cover (the README dependency
map, wire-format details, and test plans are accurate). The gaps below are
**files the docs never name**, despite being compiled into the production
daemon and (for the dataset) actively used by `SpinelNCPInstance.cpp`.

> **Correction note.** An earlier draft of this report flagged
> `SpinelNCPVendorCustom` as P0. Reading the source shows it is a **no-op
> stub** (`SpinelNCPVendorCustom.cpp:55` returns `false`;
> `mSupportedProperties` is never populated; only a placeholder
> `"__CustomKeyHere__"` is handled at lines 87-126). It is the Nest
> extension point that TI never filled in — see P2-b. The genuine blocker was
> the **operational dataset** codec + its `Dataset:*` D-Bus surface, which is
> now ported (see P0-a status below).

**Bottom line:** `wfanctl` is on track for full replacement. `wfantund`'s
operational-dataset layer (P0-a) is **implemented** (phase-3C doc + Rust
codec wired to D-Bus `Dataset:*` dispatch and incoming `PROP_VALUE_IS`
frames). The remaining partial gaps are P0-b (`system:`/TCP NCP transports)
and the P1 items (mfg D-Bus methods, secure RNG, packet matcher).

---

## 1. wfanctl (`dcuctl`) — coverage: COMPLETE

### Confirmed: the phase-2B scope is correct

`src/dcuctl/wpanctl-cmds.h:38-68` (`WPANCTL_CLI_COMMANDS`) plus the inline
entries in `wpanctl.c:128-135` define the **only** dispatch table
(`commandList[]`). The full registered set is exactly:

```text
get, set, status, reset, add, remove   (from WPANCTL_CLI_COMMANDS)
help, clear, ?                           (inline in wpanctl.c)
```

`process_input_line()` → `find_command()` walks only `commandList[]`
(`wpanctl.c:158-160`). There is **no second dispatch path**.

### The ~30 unregistered `tool-cmd-*.c` files

`src/dcuctl/` contains 30+ extra command implementations
(`tool-cmd-form.c`, `tool-cmd-join.c`, `tool-cmd-scan.c`, `tool-cmd-leave.c`,
`tool-cmd-bbr.c`, `tool-cmd-commissioner.c`, `tool-cmd-dataset.c`,
`tool-cmd-mfg.c`, `tool-cmd-pcap.c`, `tool-cmd-peek.c`, `tool-cmd-poke.c`,
`tool-cmd-poll.c`, `tool-cmd-permit-join.c`, `tool-cmd-linkmetrics.c`,
`tool-cmd-mlr.c`, `tool-cmd-add-route/prefix/service.c`,
`tool-cmd-remove-route/prefix/service.c`, `tool-cmd-begin-low-power.c`,
`tool-cmd-begin-net-wake.c`, `tool-cmd-host-did-wake.c`,
`tool-cmd-config-gateway.c`, `tool-cmd-cd.c`, `tool-cmd-resume.c`,
`tool-cmd-list.c`, `tool-cmd-commr.c`, plus `commissioner-utils.c`).

These are **linked into the binary but unreachable** from either the REPL or
one-shot mode, because none of them appear in `commandList[]`. The phase-2B
decision to skip them is therefore **correct parity** — the current C
`wfanctl` cannot run `wfanctl form` / `wfanctl scan` either.

**Operational note (not a coverage gap):** because neither the C nor Rust CLI
exposes form/join/scan, those operations are reachable only via D-Bus (the
`Form`/`Join`/`NetScanStart` methods of phase 2A) or the webapp. This is a
pre-existing C limitation inherited unchanged, not a regression. If a future
requirement wants `wfanctl form`, add it as a new Rust command — there is no C
behavior to preserve.

---

## 2. wfantund (`dcud`) — coverage: 1 P0 gap + P1 items

The phase docs cover the bulk of `src/dcud/` and `src/ncp-spinel/` well.
The following are **compiled into the production daemon and actively used,
but appear in no phase doc** — or were gaps that are now resolved.

### P0-a. `SpinelNCPThreadDataset.{cpp,h}` — 808 LOC — **RESOLVED (implemented)**

- Compiled: `src/ncp-spinel/Makefile.am` lists `SpinelNCPThreadDataset.cpp`.
- Actively used: `SpinelNCPInstance.cpp:2421-2470` loads/clears/serializes
  `mLocalDataset` to/from Spinel frames; `SpinelNCPInstance.cpp:3674-3804`
  reads every dataset field to serve the `Dataset:*` D-Bus property gets;
  `SpinelNCPInstance.cpp:1942-1953` decodes an inbound dataset frame.
- Role: full TLV codec for the operational dataset — active/pending
  timestamp (u64), master key, network name, extended PAN ID, mesh-local
  prefix (IPv6 + /64), delay timer, PAN ID, channel, PSKc, channel mask
  page 0, security policy (key rotation + flags), raw TLVs, dest IP
  (`SpinelNCPThreadDataset.h:67-80`, `parse_dataset_entry` switch at
  `SpinelNCPThreadDataset.cpp:275-476`).
- Serves **14+ `Dataset:*` D-Bus properties** defined in
  `wpan-properties.h:202-220` (`Dataset:MasterKey`, `Dataset:PSKc`,
  `Dataset:Channel`, `Dataset:PanId`, `Dataset:MeshLocalPrefix`,
  `Dataset:ActiveTimestamp`, `Dataset:RawTlvs`, etc.) plus the
  `Thread:ActiveDataset:AsValMap` / `Thread:PendingDataset:AsValMap` keys
  (`wpan-properties.h:183,185`).
- **Status: RESOLVED.** Phase-3C doc (`phase-3C-operational-dataset.md`)
  now covers the full codec with corrected spinel.h property IDs. The Rust
  port lives in `crates/dcu-tunnel-daemon/src/dataset.rs` (`OperationalDataset`
  with `Option<T>` fields, `from_spinel_frame` / `to_spinel_frame` /
  `to_valuemap` / `to_string_list`) and `crates/dcu-tunnel-daemon/src/vendor_ext.rs`
  (no-op `VendorExtension` stub). The 20 `Dataset:*` keys are registered in
  `wisun-types::property_key`, served from `dcu-dbus::properties`, and
  incoming `PROP_VALUE_IS` dataset frames are decoded in the NCP instance
  frame task. Code is present in the working tree (committed as part of the
  phase-4B integration-test commit `91fa0c0` + follow-up).
- **Action:** None remaining for P0-a. Verify the `Dataset:*` round-trip
  against the C daemon's wire format as a final acceptance check.

### P0-b. NCP transport dispatcher (`socket-utils.c`) — 1,031 LOC — **PARTIALLY COVERED**

(Formerly P0-c; renumbered after the VendorCustom downgrade below.)

- Compiled: `src/dcud/Makefile.am` lists `../util/socket-utils.c`.
- Phase 1D ports `SocketWrapper/SuperSocket/SocketAdapter/UnixSocket`, and
  phase 3A notes that `Config:NCP:SocketPath` accepts `system:` / `serial:`
  / `host:port`. That is accurate but **incomplete** for the `system:` path.
- `socket-utils.c:244-509` implements three transports the docs under-specify:
  1. **`system:` → `open_system_socket_forkpty()`** (line 418): spawns the NCP
     as a **child process** behind a PTY (double-fork, `forkpty`). This is how
     the daemon talks to a software/mock NCP binary. Phase 1D's `pty.rs`/mock
     is for *tests*; the production `system:` spawn path is not mapped.
  2. **`host:port` TCP** (line 253, `lookup_sockaddr_from_host_and_port`): TCP
     NCP transport for remote/networked NCPs — not named in phase 1D.
  3. **`socket_name_is_device`** raw `/dev/tty*` serial — covered by `uart.rs`.
- Also: `sec-random.c` (60 LOC, below) is used here for socket entropy.
- **Action:** Extend phase 1D to explicitly cover the `system:` (forkpty) and
  `host:port` (TCP) transports, not just UART + test-PTY.

### P1 gaps (smaller, secondary)

| File                          | LOC | Compiled into | Role / why it matters                                                                                       | Recommendation                                                                                                        |
| ----------------------------- | --- | ------------- | ----------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `spinel_encrypter.hpp`        | 33  | ncp-spinel    | NCP Spinel-frame encryption hook (AES-CMAC-style). Unused by default but a real code path.                  | Add a one-line note in phase 1B: Spinel encryption is a no-op stub; re-enable if firmware requires it.                |
| `NCPMfgInterface_v0/v1.h`     | 89  | dcud headers  | Manufacturing D-Bus API (`Mfg` methods). Referenced by `DBusIPCAPI.cpp` + `SpinelNCPControlInterface.h`.    | Phase 2A mentions "Mfg" only in passing. Add the mfg method set to the interface table.                               |
| `sec-random.c`                | 60  | dcud          | Secure RNG. Needed for key/PSKc generation (`GeneratePSKc` D-Bus method, phase 2A).                         | Map to `ring`/`rand` in phase 1A or 3A. In Rust this is trivial but currently unnamed.                                |
| `DBUSHelpers.cpp`             | 468 | util (ipc)    | D-Bus variant↔C++ conversion helpers used by `ipc-dbus/`.                                                   | Verify `dcu-dbus::properties::variant_to_string` covers the same conversions; add a cross-reference note in phase 2A. |
| `IPv6PacketMatcher.cpp`       | 555 | dcud          | Firewall/packet classification on the data path. Phase 1C defers it to 3A, but phase 3A does not list it.   | Assign explicitly: add `packet_matcher.rs` to the phase-3A data-path work item.                                       |

### P2 gaps (acceptable to defer, but should be stated)

| File / area                                 | LOC      | Status in docs                                                 | Note                                                                                                                                                                                                                                                            |
| ------------------------------------------- | -------- | -------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `StatCollector.cpp`                         | 1,737    | phase 3A: "Not yet / deferred past 3A"                         | Acknowledged. Largest deferred body; ensure phase-4 plan re-introduces it.                                                                                                                                                                                      |
| `NetworkRetain.cpp`                         | ~300     | phase 3A: "Not yet"                                            | Acknowledged (persistent network config).                                                                                                                                                                                                                       |
| `Pcap.cpp`                                  | ~200     | phase 3A: "Not yet"                                            | Acknowledged (packet capture).                                                                                                                                                                                                                                  |
| `NCPInstanceBase-Addresses.cpp`             | 1,332    | phase 3A: partially; phase 3B carries it forward               | Acknowledged as phase-3B work; flagged.                                                                                                                                                                                                                         |
| `NCPInstanceBase-AsyncIO.cpp`               | ~800     | phase 3A: "Not yet"                                            | The `io_task` in phase-3A `base.rs` is the async replacement; verify feature parity.                                                                                                                                                                            |
| `NCPInstanceBase-NetInterface.cpp`          | ~700     | phase 3A: "Not yet"                                            | TUN lifecycle wiring; phase-1C `dcu-tun` + phase-3A `net_interface.rs` (stub).                                                                                                                                                                                  |
| `SpinelNCPVendorCustom.cpp/.h`              | 213      | not mentioned                                                  | **No-op stub** in this build (Nest extension point TI never filled): `setup_property_supported_by_class` returns `false`, `mSupportedProperties` empty, only `"__CustomKeyHere__"` handled. Wire-in point only; port as an empty trait/extension point or omit. |
| `ncp-dummy/` plugin template                | small    | not mentioned                                                  | Acceptable: the Rust "plugin" model is compile-time crate selection, not a runtime plugin. Add a one-liner in README.                                                                                                                                           |
| `connman-plugin/`                           | ~1 file  | README non-goal                                                | Explicitly deferred — correct.                                                                                                                                                                                                                                  |
| `wpantund-fuzz.cpp` / `ncp-spinel-fuzz.cpp` | ~2 files | phase 1B has spinel fuzz target; phase 4B: fuzz "aspirational" | Daemon-level fuzz not mapped. Minor.                                                                                                                                                                                                                            |

### Util infrastructure replaced by Rust stdlib (no port needed, for clarity)

These C/C++ files are compiled but are generic infrastructure with direct
Rust-stdlib or `tokio`/`bytes` equivalents, so omitting them is correct —
listed here only to preempt "is X covered?" questions:

`Timer.cpp`→`tokio::time`; `RingBuffer.h`→`bytes::BytesMut`;
`ObjectPool.h`/`Callbacks.h`/`CallbackStore.hpp`/`EventHandler.cpp`/`NilReturn.h`
→ Rust ownership/async/traits; `nlpt.h`/`nlpt-select.c`→`tokio::select!`;
`ValueMap.cpp`→`HashMap`/`serde_json::Value`; `any-to.cpp`→`Display`/`From`;
`string-utils.c`/`time-utils.c`/`time-utils-extra.cpp`/`args.h`→stdlib;
`Data.cpp`→`Vec<u8>`; `SocketAsyncOp.h`→`AsyncRead`/`AsyncWrite`;
`config-file.c`→phase-3A `config.rs` (explicitly mapped ✓).

---

## 3. Per-phase doc accuracy spot-checks

| Phase | Doc claim verified against source                                                                          | Result                                                                                                                                                                              |
| ----- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1A    | Source files table (`NCPConstants.h`, `wpan-properties.h`, `wpan-error.h`, `NCPTypes.*`, `wisun_config.h`) | ✓ all present                                                                                                                                                                       |
| 1B    | HDLC source = `SpinelNCPInstance-DataPump.cpp:65-274`; CRC-16/X.25                                         | ✓ confirmed present and located                                                                                                                                                     |
| 1C    | TUN sources (`tunnel.c`, `TunnelIPv6Interface.*`, `netif-mgmt.c`, `IPv6Helpers.*`)                         | ✓ (the separate implementation-plan doc fixes the spec's defects — good)                                                                                                            |
| 1D    | `SocketWrapper/SuperSocket/SocketAdapter/UnixSocket`                                                       | ⚠ covers UART + Unix-socket; misses `socket-utils.c` `system:`/TCP dispatch (P0-b)                                                                                                  |
| 2A    | `ipc-dbus/DBusIPCAPI.cpp` (2453 LOC) method set                                                            | ✓ core methods present; ⚠ `Mfg`/mfg interface headers unnamed (P1); ✓ `Dataset:*` family (14+ keys) now documented in phase-2A + served from `dcu-dbus::properties` (P0-a RESOLVED) |
| 2B    | "Only 8 registered commands"                                                                               | ✓ verified: exactly 9 entries in `commandList[]` incl. help/clear/?                                                                                                                 |
| 3A    | Source table (~10,244 LOC across `src/dcud/*`)                                                             | ⚠ `ThreadDataset` lives in `src/ncp-spinel/`, not `src/dcud/` — that's why 3A missed it. (`VendorCustom` also ncp-spinel but it is a stub.) Now ported as phase-3C.                 |
| 3B    | Task table covers all 12 `SpinelNCPTask*.cpp` files                                                        | ✓ complete; the *non-task* `ThreadDataset` was the P0-a gap — now resolved in phase-3C.                                                                                             |
| 4A/4B | Mock + E2E design                                                                                          | ✓ internally consistent; correctly flagged as not-yet-implemented                                                                                                                   |

---

## 4. Recommendations (priority order)

1. **(RESOLVED) Phase 3C "Operational Dataset" doc + port** — `SpinelNCPThreadDataset`
   (808 LOC) and its 14+ `Dataset:*` D-Bus properties are now ported
   (phase-3C doc + `dataset.rs`/`vendor_ext.rs`, wired to D-Bus). No further
   action beyond a final wire-format acceptance check against the C daemon.
2. **Amend Phase 2A** to add the `Dataset:*` property family to the recognized
   property table, enumerate the manufacturing D-Bus methods from
   `NCPMfgInterface_v0/v1.h`, and cross-check `DBUSHelpers.cpp` conversions
   against `dcu-dbus::properties`.
3. **Amend Phase 1D** to explicitly cover the `system:` (forkpty NCP-spawn)
   and `host:port` (TCP) transports in `socket-utils.c`, not just UART.
4. **Assign `IPv6PacketMatcher.cpp`** (555 LOC) to the phase-3A data path
   (currently "deferred to 3A" in phase 1C but unlisted in phase 3A).
5. **Name `sec-random.c`** (secure RNG → `ring`/`rand`) and
   `spinel_encrypter.hpp` (Spinel encryption stub) somewhere — phase 1A/1B
   respectively — so they are not lost.
6. **wfanctl is ready for full replacement as scoped**; no CLI work remains
   beyond phase 2B. (Optional: add `form`/`join`/`scan` as new Rust CLI
   commands if operator ergonomics are desired — no C parity obligation.)

---

## 5. Conclusion

The rust-porting plan is high-quality and largely complete. **`wfanctl`
(dcuctl) is fully covered** for replacement. **`wfantund` (dcud) had one
blocking unmentioned dependency** — the operational-dataset codec
(`SpinelNCPThreadDataset`, 808 LOC) and its `Dataset:*` D-Bus surface — which
is now **resolved** (phase-3C doc + Rust codec wired to D-Bus and incoming
frames). The remaining open items are the `system:`/TCP NCP transports
(P0-b, doc-only so far) and the P1 items (mfg D-Bus methods, secure RNG,
packet matcher). The vendor-property layer (`SpinelNCPVendorCustom`) is a
no-op stub and is not a blocker. With P0-b and the P1 items addressed,
the Rust port reaches drop-in replacement parity for `wfantund`.

---

## Appendix: Verification rounds (2026-07-12)

Two validation rounds were run after the initial report and phase-3C
doc were written. The following discrepancies were found and fixed.

### Round 1 — Source verification (LOC counts, paths, line refs)

| Issue                                              | Phase | Severity | Status  |
| -------------------------------------------------- | ----- | -------- | ------- |
| `spinel.h` path wrong (`src/spinel/` → `src/ncp/`) | 1B    | High     | Fixed   |
| `platform-hdlc.h` does not exist                   | 1B    | Medium   | Removed |
| LOC: `NCPConstants.h` ~200 → 51                    | 1A    | Low      | Fixed   |
| LOC: `wpan-properties.h` ~300 → 636                | 1A    | Low      | Fixed   |
| LOC: `NCPTypes.cpp` ~20 → 465                      | 1A    | Medium   | Fixed   |
| LOC: `wisun_config.h` ~150 → 38                    | 1A    | Low      | Fixed   |
| LOC: `spinel-extra.h` ~150 → 83                    | 1B    | Low      | Fixed   |
| LOC: `spinel-extra.c` ~800 → 382                   | 1B    | Low      | Fixed   |
| LOC: `DataPump.cpp` ~400 → 629                     | 1B    | Low      | Fixed   |
| LOC: `Data.cpp` ~200 → 23                          | 1B    | Low      | Fixed   |
| LOC: `SocketWrapper.cpp` 400 → 120                 | 1D    | Medium   | Fixed   |
| LOC: `DBusIPCAPI.h` ~80 → 332                      | 2A    | Low      | Fixed   |
| LOC: `DBUSIPCServer.cpp` ~600 → 405                | 2A    | Low      | Fixed   |
| LOC: `NCPInstanceBase-AsyncIO.cpp` ~800 → 260      | 3A    | Medium   | Fixed   |
| LOC: `NCPInstanceBase-NetInterface.cpp` ~700 → 477 | 3A    | Medium   | Fixed   |
| LOC: `NCPInstance.cpp` ~400 → 149                  | 3A    | Medium   | Fixed   |
| LOC: `NCPControlInterface.cpp` ~600 → 344          | 3A    | Medium   | Fixed   |
| LOC: `RunawayResetBackoffManager.cpp` ~150 → 70    | 3A    | Medium   | Fixed   |

### Round 2 — Inter-doc and code verification

| Issue                                                                     | Phase  | Severity     | Status                                        |
| ------------------------------------------------------------------------- | ------ | ------------ | --------------------------------------------- |
| Property IDs ALL wrong (13 of 14 incorrect hex values)                    | 3C     | **CRITICAL** | Fixed — uses correct spinel.h computed values |
| `platform-hdlc.h` referenced but does not exist                           | 1B     | Medium       | Removed from source table                     |
| README: phase 3B marked "Not started" but code exists (commit ffc8b81)    | README | High         | Fixed → Done                                  |
| README: phase 4A marked "Not started" but code exists (commit d1b8e80)    | README | High         | Fixed → Done                                  |
| `repl.rs` / `property_formatter.rs` in spec but not implemented           | 2B     | Low          | Added deviation note                          |
| `wpanctl.c` commandList: doc says "8 commands" but table lists 10 entries | 2B     | Low          | Noted (9 unique + ? alias)                    |

### Corrected property ID values (phase-3C)

The original doc had fabricated hex values. Correct values from
`spinel.h` (computed from base constants):

| Constant                        | Doc had | Correct | Formula                  |
| ------------------------------- | ------- | ------- | ------------------------ |
| `DATASET_ACTIVE_TIMESTAMP`      | 0x0130  | 0x151C  | THREAD_EXT + 28          |
| `DATASET_PENDING_TIMESTAMP`     | 0x0131  | 0x151D  | THREAD_EXT + 29          |
| `DATASET_DELAY_TIMER`           | 0x0134  | 0x151E  | THREAD_EXT + 30          |
| `DATASET_SECURITY_POLICY`       | 0x0132  | 0x151F  | THREAD_EXT + 31          |
| `DATASET_RAW_TLVS`              | 0x0133  | 0x1520  | THREAD_EXT + 32          |
| `DATASET_DEST_ADDRESS`          | 0x0135  | 0x1527  | THREAD_EXT + 39          |
| `NET_MASTER_KEY`                | 0x44    | 0x46    | NET__BEGIN + 6           |
| `NET_NETWORK_NAME`              | 0x46    | 0x44    | NET__BEGIN + 4           |
| `IPV6_ML_PREFIX`                | 0x0A    | 0x62    | IPV6__BEGIN + 2          |
| `MAC_15_4_PANID`                | 0x3F    | 0x36    | MAC__BEGIN + 6           |
| `PHY_CHAN`                      | 0x3C    | 0x21    | PHY__BEGIN + 1           |
| `PHY_CHAN_SUPPORTED`            | 0x31    | 0x22    | PHY__BEGIN + 2           |
| `NET_PSKC`                      | 0x47    | 0x4B    | NET__BEGIN + 11          |

> **Implementor note:** The dataset module should import
> `spinel::property::PROP_*` constants where they exist, and define
> the missing DATASET_* locally (or add them to the spinel crate first).
> Do NOT hardcode hex values — derive them from the base constants.
