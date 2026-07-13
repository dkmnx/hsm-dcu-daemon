# Rust Porting Coverage Gap Analysis

**Date:** 2026-07-13 (re-verified same day)  
**Confidence:** High on **presence/absence** of modules and D-Bus wiring
(grep + source read). Medium on **product-critical priority** (what a given
deployment actually needs) — that needs product owner input, not only LOC.  
**Scope:** True remaining gaps between the C production binaries
(`wfantund`, `wfanctl`) and the Rust port (`dcutund`, `dcuctl`), and the
work required for **wfantund drop-in parity**.  
**Method:** Enumerated every file compiled by
`src/wfantund/Makefile.am`, `src/ncp-spinel/Makefile.am`,
`src/wfanctl/Makefile.am`, and `src/ipc-dbus/*`, then verified presence
and *wiring* in `crates/*` (crate exists ≠ production path uses it).
D-Bus methods were diffed programmatically: every `WPANTUND_IF_CMD_*` in
`wpan-dbus.h` vs `INTERFACE_CALLBACK_CONNECT` in `DBusIPCAPI.cpp` vs
`#[zbus(name = …)]` in `crates/dcu-dbus/src/interface.rs`.

> **Binary names (this tree)**  
> | Role | C (autotools) | Rust (Cargo) |
> | ---- | ------------- | ------------ |
> | Daemon | `sbin_PROGRAMS = wfantund` | `[[bin]] name = "dcutund"` |
> | CLI | `bin_PROGRAMS = wfanctl` | package `dcuctl` |
> | Interface | `wfan0` | `wfan0` (default) |
> | Config | `/etc/wpantund.conf` | `/etc/wpantund.conf` (default `-c`) |
>
> In this report **wfantund ≡ dcutund** and **wfanctl ≡ dcuctl** for
> functional comparison. Packaging must install or symlink the expected
> names for a silent field swap.

### Logical port — not a direct / line-for-line port

**This project is a logical port: re-implement observable behavior in
idiomatic Rust. It is not a mechanical translation of the C++ sources.**

C `wfantund` and Rust `dcutund` should look the same from outside (config
file, D-Bus names/methods, Spinel wire, TUN/routing outcomes). They will
**not** share internal structure. C patterns (protothreads, `select`,
callbacks, `boost::any`, deep class hierarchies, hand-rolled buffers)
map poorly to Rust conventions (ownership, `async`/`await`, typed errors,
traits, crates, limited `unsafe`).

| Preserve (external contract)                                     | Do **not** preserve (internal form)                                                        |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Spinel / HDLC wire with TI NCP firmware                          | Protothread / `nlpt` control flow → use Tokio tasks + `select!`                            |
| D-Bus well-known name, paths, method/signal **names**            | Nested C++ virtual tables / IPCServer class layout                                         |
| Property key strings and (where clients depend) value formatting | `boost::any` / ad-hoc maps → typed state + explicit codecs                                 |
| `wpantund.conf` keys that production uses                        | 1:1 file names (`NCPInstanceBase-Addresses.cpp` need not become one giant `addresses.cpp`) |
| Daemon lifecycle effects (pidfile, priv-drop, retain, TUN up)    | `goto bail` / `require_action` macros → `Result` / `?`                                     |
| CLI command surface of registered `wfanctl` cmds                 | Unreachable dead `tool-cmd-*.c` code                                                       |

**Implications for this gap analysis:**

1. **A “gap” means missing external behavior**, not “this `.cpp` has no
   twin `.rs`.” C source paths are **references** for behavior, not a
   required module tree.
2. **Closing a gap** means matching the contract (tests, golden D-Bus/
   Spinel, field checks)—not copying C control flow or class graphs.
3. **Rust-native design is preferred** when it still meets the contract
   (e.g. `zbus` instead of libdbus; `thiserror` instead of errno soup;
   `dcu-serial`/`dcu-tun` crates instead of a single mega-binary).
4. **LOC ratios are misleading.** ~1.3k LOC of C addresses code may
   become less or more Rust; size is not the acceptance criterion.
5. **Intentional internal deviations are expected** (e.g. stringified
   `PropGet` via zbus constraints—document wire impact, don’t reintroduce
   C++ variants for purity).

**Implementor rule of thumb:** read C to learn *what* must happen on the
wire and to clients; write Rust the way a Rust daemon should be written.
If a PR looks like C with Rust syntax, reject it.

### PR-review guardrails — concrete signs of line-by-line porting

Use this checklist when reviewing a gap-closing PR. A PR that scores
“yes” on any **Do not merge** item is copying form, not preserving
contract.

| Do merge (logical port)                                                                                   | Do not merge (line-by-line port)                                                                  |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Adds a Rust-native module whose public methods are driven by D-Bus/Spinel events, not by C++ method names | Renames C++ classes to `struct Foo` and keeps virtual-method-shaped dispatch                      |
| Replaces protothreads with Tokio tasks/`select!` and typed errors                                         | Introduces `nlpt`-style state machines or hand-rolled `goto` loops                                |
| Uses `zbus` object server for D-Bus; one object per logical path                                          | Manually reimplements libdbus message dispatch to mirror `DBusIPCAPI.cpp`                         |
| Property handlers return strongly-typed Rust values, then convert at the API boundary                     | Carries `boost::any` / string-keyed maps deep into the daemon                                     |
| TUN bridge is async I/O between two independent streams                                                   | Copies `nlpt` read/write state machine verbatim                                                   |
| Tests verify wire behavior (D-Bus signature, Spinel payload, TUN packet)                                  | Tests only assert “same LOC as C” or compare C++ structs                                          |
| Deletes/ignores dead C code (unregistered CLI commands, no-op vendor stubs)                               | Ports unreachable `tool-cmd-*.c` files or empty `SpinelNCPVendorCustom` bodies “for completeness” |

**Owner sign-off required before intentionally preserving C structure:**
if a PR’s justification is “C does it this way,” it must also explain why
that structure is externally visible or why a Rust-native design cannot
meet the contract.

### How to read “parity” (external, not structural)

| Tier                             | Meaning                                                                                                                  | Use this for                                |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------- |
| **T1 — Field drop-in**           | System bus + GetInterfaces + TUN/addresses + form/join/status/get/set + packaging names; clients (`dcuctl`, webapp) work | Shipping replacement                        |
| **T2 — Behavioral completeness** | Every **registered** D-Bus method C exposes, Stat/Pcap/Retain, production property set on target firmware                | Full client/ops compatibility with C daemon |

“Parity” here means **behavior and contracts**, not isomorphic source.
P0 items are **T1 blockers**. P1 items complete **T2** (and some T1
hardening). Do not treat “~325 property defines” as a blind implementor
checklist — many are Thread-era; inventory against live C `status` /
firmware first (P1-7).

### Implementor readiness of *this document*

| Ready now?                      | What this doc is                                                      | What it is not                                                                                          |
| ------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Yes as backlog / triage**     | Verified gap list, priorities, C file pointers, acceptance milestones | —                                                                                                       |
| **Not alone as design tickets** | —                                                                     | Per-gap wire-format specs, zbus signatures, TID state-machine detail, effort estimates, golden fixtures |
| **Not a C→Rust file map**       | C paths cite *behavior to re-implement*                               | A mandate to recreate C module layout                                                                   |

**Before coding each P0/P1 item**, write a short implementor note (or
amend the matching phase-*.md) with: external contract (D-Bus/Spinel/
config), C entry points as reference, idiomatic Rust design, files to
touch, tests. Milestone A (P0-1/P0-2) is small enough to implement from
this doc + `DBUSIPCServer.cpp` alone.

---

## Executive summary

| Target | Drop-in today? | Verdict |
| ------ | -------------- | ------- |
| **wfanctl → dcuctl** | **Yes** (CLI surface) | All 9 registered C commands ported. Same D-Bus client contract. Rename/package only (see P1-9). **Runtime unblocked after P0-1/P0-2 closed** — now works against a `dcutund` on the system bus. |
| **wfantund → dcutund** | **Partial** (Milestones A+B) | Core Spinel/D-Bus/task stack exists and mock-e2e passes. System bus, base object, TUN data path, and address/prefix/route manager are implemented. Remaining gaps: NetworkRetain, lifecycle, GPIO, auto-flags, StatCollector, Pcap, 13 D-Bus methods, NetworkTimeUpdate, property inventory. |

### Headline blockers for T1 field drop-in (P0)

| ID | Gap | Evidence (re-verified 2026-07-13) | Status |
| -- | --- | -------------------------------- | ------ |
| **P0-1** | **System D-Bus bus** | C: `DBUSIPCServer.cpp:68` `DBUS_BUS_SYSTEM`. Rust: `dcu-dbus/src/server.rs` `Connection::system()` via `BusType::System` (default) + `DCU_DBUS_BUS` env override; `start_with_bus` added. **Closed** (6d1f72d) | DONE |
| **P0-2** | **Base object `GetInterfaces`** | C: `DBUSIPCServer.cpp` ~286– registers base path method. Rust: `base_interface.rs` registers `BaseInterface` at `WPANTUND_BASE_OBJECT_PATH` with `GetInterfaces` (`aas`) + `GetVersion` (`u`); per-iface `GetVersion` removed from `interface.rs`. `dcuctl` calls `GetVersion` on base proxy. **Closed** (6d1f72d) | DONE |
| **P0-3** | **TUN data path** | C: `SpinelNCPInstance.cpp` ~6617+ `SPINEL_PROP_STREAM_NET`. Rust: `start_pumps_impl` opens TUN (`dcu-tun::TunnelIPv6Interface`) + spawns `ncp_to_tun`/`tun_to_ncp` bridge tasks; `set_ncp_state` brings TUN up on interface-up. `crates/instance/tun_bridge.rs`. Added multicast join/leave (ioctl + interface). Spinel constants `PROP_STREAM_NET`/`_INSECURE`/`_RAW`/`_DEBUG` corrected against spec. **Closed** | DONE |
| **P0-4** | **IPv6 address / prefix / route manager** | C: `NCPInstanceBase-Addresses.cpp` (~1332 LOC). Rust: `crates/instance/addresses.rs` — `AddressManager` with origin tracking (NCP/Interface/User), `apply_ncp_address_table`/`multicast`/`on_mesh`/`off_mesh` methods diffing full-table snapshots and returning `TunOp` vecs. NCP table frames forwarded from frame-task to main loop via channel; `apply_tun_ops` under write lock. `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes` served from DaemonState mirror. `PropInsert`/`PropRemove` for prefix/route keys updates AddressManager immediately + NCP push. **Closed** | DONE |
| **P0-5** | **NetworkRetain** | Only string `"Config:Daemon:NetworkRetainCommand"` in `property_key.rs`. No runtime. C: `NetworkRetain.cpp` (~215 LOC). | OPEN |

### Secondary gaps (P1) — T2 / ops completeness

| ID       | Gap                                     | C source                                                  | Notes                                                          |
| -------- | --------------------------------------- | --------------------------------------------------------- | -------------------------------------------------------------- |
| **P1-1** | StatCollector + `Stat:*`                | `StatCollector.cpp` (~1.7k)                               | Large; may be optional for some deployments                    |
| **P1-2** | Pcap + `PcapToFd` / `PcapTerminate`     | `Pcap.cpp` + registered in `DBusIPCAPI`                   | Registered in C — real gap                                     |
| **P1-3** | Missing **registered** D-Bus methods    | See §2.3 method matrix                                    | 13 methods; **not** PermitJoin/BeginNetWake (header-only in C) |
| **P1-4** | PID file, priv-drop, chroot             | `wpantund.cpp`                                            | Config parsed, not applied                                     |
| **P1-5** | Hard-reset / power GPIO paths           | config keys                                               | Parsed only                                                    |
| **P1-6** | AutoDeepSleep / AutoAssociateAfterReset | config flags                                              | Parsed only                                                    |
| **P1-7** | Property surface vs production set      | `wpan-properties.h` (~325 defines) vs ~40 Spinel handlers | **Inventory first**; do not blind-port all 325                 |
| **P1-8** | `NetworkTimeUpdate` signal              | C connects `mOnNetworkTimeUpdate`                         | Missing in Rust signals                                        |
| **P1-9** | Binary / packaging names                | Makefile vs Cargo                                         | Symlink OK                                                     |

### Known intentional / already-documented deviations (not “missing code”)

| Item                  | Reality                                                                                                                 | Implementor note                                                                                      |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `PropGet` return type | C returns typed **variant** via callback helper; Rust returns **stringified** `String` (`interface.rs` + phase-2A note) | Clients that only print strings (dcuctl, much of webapp PropGet) OK; typed-variant clients need audit |
| `Status` dict values  | Rust `HashMap<String, String>`; C builds variant dict                                                                   | Same stringification caveat                                                                           |
| Joiner task file      | `tasks/joiner_commission.rs` exists                                                                                     | **Not** exposed as D-Bus methods yet (P1-3)                                                           |

### Acceptable non-blockers (P2)

| Item                                       | Rationale                                                                                 |
| ------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `SpinelNCPVendorCustom`                    | No-op stub in this build; Nest extension never filled by TI                               |
| `connman-plugin/`                          | Explicit README non-goal                                                                  |
| Porting `ti-wisun-webapp` source           | Explicit non-goal; **webapp as a client** still requires T1 D-Bus (system bus)            |
| `ncp-dummy/` runtime plugin                | Rust uses compile-time crates, not `.so` plugins                                          |
| `spi-hdlc-adapter`                         | Separate OpenThread helper; reachable via `system:` transport if installed                |
| `spinel_encrypter.hpp`                     | Optional encryption hook; unused by default                                               |
| Unregistered `wfanctl` tool-cmd-\*.c files | Linked but **not** in `commandList[]` — C cannot run them either                          |
| `PermitJoin`, `BeginNetWake` D-Bus cmds    | Defined in `wpan-dbus.h` but **not** `INTERFACE_CALLBACK_CONNECT`’d in C — not a Rust gap |

---

## 1. wfanctl (`dcuctl`) — COMPLETE

### Registered command surface

C dispatch is **only** `commandList[]` in `wfanctl.c` (via `WPANCTL_CLI_COMMANDS` + inline help/clear/?). Registered set:

```text
get, set, status, reset, add, remove, help, clear, ?
```

Rust `dcuctl` implements the same set (plus REPL quit aliases `quit`/`exit`/`q`).

### Unreachable C tool commands

~30 `tool-cmd-*.c` files (form, join, scan, dataset, commissioner, pcap, …) are **compiled into the C binary but never registered**. Skipping them in Rust is **correct parity**, not a gap. Form/join/scan remain D-Bus / webapp only on both stacks.

### CLI residual packaging note

- Binary name: C `wfanctl` vs Rust `dcuctl` — install symlink if scripts expect `wfanctl`.
- `dcuctl` already targets system bus + `GetInterfaces`; it will only work against a daemon that provides both (see P0-1, P0-2).

**Verdict:** CLI surface is done. No further command work is required for wfanctl parity.

---

## 2. wfantund (`dcutund`) — implemented vs missing

### 2.1 Implemented (phases 1A–4B) — verified present

| Area                             | Rust location                  | Notes                                                                                                                       |
| -------------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Types / property key constants   | `crates/wisun-types`           | Includes secure RNG, dataset-related keys                                                                                   |
| Spinel codec + HDLC              | `crates/spinel`                | Fuzz target present                                                                                                         |
| TUN library                      | `crates/dcu-tun`               | Device, ioctl, packet matcher, multicast — **wired into daemon**                                                            |
| TUN ↔ NCP bridge                 | `instance/tun_bridge.rs`       | `ncp_to_tun` + `tun_to_ncp` async tasks (STREAM_NET/INSECURE)                                                               |
| Address / prefix / route manager | `instance/addresses.rs`        | `AddressManager` with origin tracking, NCP table sync, user insert/remove                                                   |
| Multicast join/leave             | `dcu-tun/ioctl.rs`             | `IPV6_JOIN_GROUP`/`IPV6_LEAVE_GROUP` via `setsockopt`                                                                       |
| Serial / TCP / `system:` / PTY   | `crates/dcu-serial`            | Transport dispatch implemented                                                                                              |
| D-Bus interface methods (subset) | `crates/dcu-dbus`              | Form/Join/Leave/Status/Prop\*/scans/Mfg/… + **IPv6:AllAddresses, IPv6:Routes, Thread:OnMeshPrefixes, Thread:OffMeshRoutes** |
| D-Bus signals (subset)           | `crates/dcu-dbus/signals.rs`   | NetScanBeacon, EnergyScanResult, PropChanged, InterfaceAdded/Removed                                                        |
| Daemon core + Spinel I/O         | `crates/dcu-tunnel-daemon`     | Response table, io_task, command dispatch                                                                                   |
| Spinel tasks                     | `tasks/*`                      | form, join, leave, scan, sleep, peek, topology, joiner_commission, …                                                        |
| Operational dataset              | `dataset.rs`                   | Codec + DaemonState mirror for `Dataset:*`                                                                                  |
| Firmware upgrade helpers         | `firmware_upgrade.rs`          | Wired when `AutoFirmwareUpdate` is set                                                                                      |
| Config parser                    | `config.rs`                    | wpantund.conf key subset                                                                                                    |
| Mock NCP + e2e                   | `dcu-mock` + integration tests | form/join/startup/timeout vs mock                                                                                           |
| Runaway reset backoff            | `tasks/backoff.rs`             | Present                                                                                                                     |

### 2.2 True gaps (detailed)

#### P0-1 — System bus (production D-Bus)

|         | C                               | Rust today                                                                                                                                  |
| ------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Bus     | `dbus_bus_get(DBUS_BUS_SYSTEM)` | `Connection::system()` via `BusType::System` (default); `DCU_DBUS_BUS=session` env override for CI. `start_with_bus` added to `DbusServer`. |
| Clients | system bus                      | `dcuctl` uses system bus                                                                                                                    |

**Status:** CLOSED (commit 6d1f72d). Production `dcutund` now claims
`com.nestlabs.WPANTunnelDriver` on the **system** bus by default.

#### P0-2 — Base object + `GetInterfaces`

C (`DBUSIPCServer.cpp`): method on
`/com/nestlabs/WPANTunnelDriver` returns `aas`-style array of
`[iface_name, unique_bus_name]` pairs.

Rust: new `base_interface.rs` registers `BaseInterface` at
`WPANTUND_BASE_OBJECT_PATH` with `GetInterfaces() -> aas`
(`[iface_name, unique_bus_name]`) and `GetVersion() -> u` (returns `2`).
The per-interface `GetVersion` was removed from `WpanInterface` (C serves
it from the base object). `dcuctl` calls `GetVersion` on the base proxy.

**Status:** CLOSED (commit 6d1f72d). Full base object tree matches C:

1. Register a base object at `WPANTUND_BASE_OBJECT_PATH`.
2. Implement `GetInterfaces` returning the same wire shape C uses.
3. Keep per-iface object at `.../wfan0` with the full method set.
4. Route `GetVersion` from the base object (C serves it there, not on the
   per-interface interface).

#### P0-3 — TUN data path

`dcu-tun` is a workspace dependency of `dcu-tunnel-daemon`, but
`start_pumps` only opens the **NCP transport** and spawns Spinel
`io_task`. There is no:

- `open` / configure TUN (`Config:TUN:InterfaceName`)
- host→NCP IPv6 packet pump
- NCP→host IPv6 delivery
- use of `packet_matcher` on the live path

**Implementation (CLOSED — Milestone B):**

| Direction    | Rust module                                           | Key files                                                                                                                                     |
| ------------ | ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| NCP → host   | `instance/tun_bridge.rs` `ncp_to_tun`                 | TUN write of RAW IPv6 from `PROP_STREAM_NET` / `_INSECURE` frames forwarded via channel from `dispatch_unsolicited_static`                    |
| Host → NCP   | `instance/tun_bridge.rs` `tun_to_ncp`                 | TUN async_read → `PROP_VALUE_SET(0x72/0x73)` with 5-byte Spinel header; insecure when NCP not associated                                      |
| TUN open     | `start_pumps_impl` via `dcu-tun::TunnelIPv6Interface` | `TunConfig{name, mtu=1280, no_packet_info=true}`                                                                                              |
| TUN up/down  | `set_ncp_state`                                       | `tun.set_up(state.is_associated())` on state transitions                                                                                      |
| Multicast    | `dcu-tun/ioctl.rs`                                    | `IPV6_JOIN_GROUP`/`IPV6_LEAVE_GROUP` via `setsockopt`                                                                                         |
| Constants    | `spinel/src/property.rs`                              | `PROP_STREAM_NET=0x72`, `PROP_STREAM_NET_INSECURE=0x73`, `PROP_STREAM_RAW=0x71`, `PROP_STREAM_DEBUG=0x70`, `PROP_MAC_RAW_STREAM_ENABLED=0x37` |

(IPv6PacketMatcher on the live path is deferred — forward-all passthrough for now.)

#### P0-4 — Address / prefix / route manager

**Implementation (CLOSED — Milestone B):**

| Capability                        | Rust module / location                                                                                                                                                              |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Origin tracking                   | `instance/addresses.rs` — `Origin::{Ncp, Interface, User}`                                                                                                                          |
| NCP-driven unicast table sync     | `AddressManager::apply_ncp_address_table` → `TunOp::AddAddress/RemoveAddress`                                                                                                       |
| NCP-driven multicast table sync   | `AddressManager::apply_ncp_multicast_table` → `TunOp::JoinMulticast/LeaveMulticast`                                                                                                 |
| NCP-driven on-mesh prefix sync    | `AddressManager::apply_ncp_on_mesh_table` → `TunOp::AddRoute/RemoveRoute` (metric 256)                                                                                              |
| NCP-driven off-mesh route sync    | `AddressManager::apply_ncp_off_mesh_table` → `TunOp::AddRoute/RemoveRoute`                                                                                                          |
| User PropInsert/PropRemove        | `address_prop_insert`/`address_prop_remove` in `base.rs` → `insert_on_mesh`/`remove_on_mesh`/`insert_off_mesh`/`remove_off_mesh`                                                    |
| D-Bus property serving            | `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes` in `properties.rs`, mirrored from AddressManager via `mirror_address_state`                     |
| TUN convergence (lock discipline) | Write lock held across ops computation + `apply_tun_ops`; release then `mirror_address_state`                                                                                       |
| Frame delivery                    | `is_address_table_frame` detects `IPV6_ADDRESS_TABLE`/`MULTICAST`/`ON_MESH_NETS`/`OFF_MESH_ROUTES` in spawned frame-task; forwarded via channel to main loop `handle_address_frame` |

#### P0-5 — NetworkRetain

C `NetworkRetain.cpp` (~215 LOC):

- `Config:Daemon:NetworkRetainCommand` opens a pipe/FD to an external helper
- On NCP state transitions: save / recall / erase network info
- Used so networks survive NCP reset without re-form

Rust: property key + config field only; **no runtime module**.

**Parity requirement:** Implement retain command lifecycle matching C
FD protocol (`save` / `recall` / `erase` / close), driven from NCP state
changes.

---

#### P1-1 — StatCollector

C exposes many `Stat:*` properties (node TX/RX history, link quality,
NCP state history, …) via `StatCollector::is_a_stat_property`.

**Parity requirement:** Port stats recording on packet + state events
and serve `Stat:*` via PropGet. Largest deferred C body (~1.7k LOC).

#### P1-2 — Pcap

C: `PcapManager` + D-Bus `PcapToFd` / `PcapTerminate`; Spinel path
pushes frames when pcap is active.

**Parity requirement:**

- D-Bus methods + FD handoff semantics matching C
- Frame capture on NCP RX/TX when enabled
- Optional: pcap file format parity with C `PcapPacket`

#### P1-3 — Missing D-Bus interface methods (registered in C only)

**Method matrix (2026-07-13 programmatic diff):**

- C header defines: **47** `WPANTUND_IF_CMD_*`
- C actually registered (`INTERFACE_CALLBACK_CONNECT`): **45**
  (excludes `PermitJoin`, `BeginNetWake` — header-only)
- Rust `#[zbus(name)]` on iface: **33** (includes `GetVersion`, which C
  serves from the base server as `WPANTUND_IF_GET_VERSION`)

**Reproduce this matrix (run before claiming a method is done):**

```bash
# C registered method names (keep underscores; C uses UPPER_SNAKE_CASE,
# e.g. WPANTUND_IF_CMD_ANNOUNCE_BEGIN → ANNOUNCE_BEGIN).
grep -oE 'WPANTUND_IF_CMD_[A-Za-z0-9_]+' src/ipc-dbus/DBusIPCAPI.cpp | \
  sed 's/WPANTUND_IF_CMD_//' | sort -u > /tmp/c.txt

# C header defines (includes unregistered aliases like PERMIT_JOIN,
# NETWORK_WAKE_BEGIN).
grep -oE 'WPANTUND_IF_CMD_[A-Za-z0-9_]+' src/ipc-dbus/wpan-dbus.h | \
  sed 's/WPANTUND_IF_CMD_//' | sort -u > /tmp/c_header.txt

# Rust zbus methods (PascalCase, e.g. AnnounceBegin).
grep -oE '#\[zbus\(name = "[^"]+"\)\]' crates/dcu-dbus/src/interface.rs | \
  sed -E 's/.*"([^"]+)".*/\1/' | sort -u > /tmp/r.txt

# Missing from Rust but registered in C. C is UPPER_SNAKE_CASE and Rust is
# PascalCase, so normalize to lower-case and strip underscores before diffing.
# (The naive `comm -23 /tmp/c.txt /tmp/r.txt` reports every C method as
# "missing" because the names never match — do NOT use it.)
comm -23 <(tr 'A-Z' 'a-z' < /tmp/c.txt  | tr -d '_') \
         <(tr 'A-Z' 'a-z' < /tmp/r.txt | tr -d '_')
```

> The counts above were produced with these commands. Re-run them before
> updating the matrix; do not hand-maintain the list. Header defines **47**
> (`/tmp/c_header.txt`), C registers **45** (excludes `PERMIT_JOIN`,
> `NETWORK_WAKE_BEGIN`), Rust implements **33** (`/tmp/r.txt`); the
> case/underscore-insensitive diff yields the **13** registered-in-C
> methods still missing from Rust listed in the table below.

**Registered in C, missing in Rust (13):**

| Method                   | Notes for implementor                                     |
| ------------------------ | --------------------------------------------------------- |
| `PcapToFd`               | Pair with P1-2; FD passing over D-Bus                     |
| `PcapTerminate`          | Pair with P1-2                                            |
| `JoinerAttach`           | See `SpinelNCPControlInterface` / joiner attach path      |
| `JoinerStart`            | Wire `tasks/joiner_commission.rs` (`action=true`)         |
| `JoinerStop`             | Wire same task (`action=false`)                           |
| `JoinerCommissioning`    | Deprecated alias in C; still registered — keep for parity |
| `JoinerAdd`              | Commissioner                                              |
| `JoinerRemove`           | Commissioner                                              |
| `LinkMetricsQuery`       | No Rust task yet; C in SpinelNCPInstance                  |
| `LinkMetricsProbe`       | No Rust task yet                                          |
| `LinkMetricsMgmtForward` | No Rust task yet                                          |
| `LinkMetricsMgmtEnhAck`  | No Rust task yet                                          |
| `EnergyScanQuery`        | Distinct from EnergyScanStart/Stop                        |

**Not a gap (C also does not register):** `PermitJoin`, `BeginNetWake`.

**Present in Rust (32 functional + GetVersion):** PropGet/Set/Insert/Remove,
Status, Form, Join, Leave, Reset, BeginLowPower, HostDidWake, Attach,
ConfigGateway, DataPoll, NetScanStart/Stop, DiscoverScanStart/Stop,
EnergyScanStart/Stop, MlrRequest, BackboneRouterConfig, AnnounceBegin,
PanIdQuery, GeneratePSKc, Mfg, Peek, Poke, RouteAdd/Remove,
ServiceAdd/Remove, GetVersion.

**Also missing at base path (P0-2):** `GetInterfaces` (base cmd, not IF cmd).

#### P1-4 — Daemon process lifecycle

Parsed in `config.rs` but **not applied** in `main.rs` / instance:

| Config key                     | C behavior                | Rust                                                       |
| ------------------------------ | ------------------------- | ---------------------------------------------------------- |
| `Config:Daemon:PIDFile`        | write PID, unlink on exit | ignored                                                    |
| `Config:Daemon:PrivDropToUser` | setgid/setuid after bind  | ignored                                                    |
| `Config:Daemon:Chroot`         | chroot + chdir            | ignored                                                    |
| syslog / log mask              | syslog                    | tracing only (acceptable if journald-compatible; document) |

**Parity requirement:** Apply PID file + optional priv-drop/chroot after
privileged setup (TUN, D-Bus name, serial open) matching C order.

#### P1-5 — Hard reset / power GPIO

`Config:NCP:HardResetPath` / `PowerPath` parsed; C toggles sysfs GPIO
values on reset/power. Rust never writes these paths.

#### P1-6 — AutoAssociate / AutoDeepSleep

Config booleans exist; C uses them in the state machine after reset /
idle. Rust does not fully automate association or deep-sleep entry from
these flags.

#### P1-7 — Property surface

|                        | C                                        | Rust                                                                                                                                                      |
| ---------------------- | ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Named property defines | ~325 in `wpan-properties.h`              | ~50–80 string keys in `property_key` macro + tests                                                                                                        |
| Spinel handler map     | large switch tables in SpinelNCPInstance | ~50 entries in `property_handlers.rs` + `handle_get_property` (incl. `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes`) |

Not every C key is active on TI Wi-SUN firmware, but **100% parity**
means:

1. Inventory keys actually returned by C `Status` / `PropGet` on target
   firmware.
2. Implement daemon-local vs NCP-forwarded handlers for that set.
3. Golden-test string formatting against C (`variant_to_string` parity).

#### P1-8 — `NetworkTimeUpdate` signal

C emits `NetworkTimeUpdate`; Rust signal helpers omit it.

#### P1-9 — Install names

For drop-in packages:

- Install `dcutund` as `/usr/sbin/wfantund` (or symlink)
- Install `dcuctl` as `/usr/bin/wfanctl` (or symlink)
- Ship `wpantund.conf` / unit files unchanged where possible

---

## 3. Phase docs vs code (staleness)

| Phase | README status               | Code                                                      | Doc accuracy note                                                |
| ----- | --------------------------- | --------------------------------------------------------- | ---------------------------------------------------------------- |
| 1A–2B | Done                        | Present                                                   | OK                                                               |
| 3A    | Done                        | Core present; addresses/netif/pcap/stat/retain still open | phase-3A still lists several “Not yet” items — correct           |
| 3B    | Done                        | Tasks present                                             | OK                                                               |
| 3C    | “Implemented (uncommitted)” | `dataset.rs` present                                      | **phase-3C header still says “Not started”** — update separately |
| 4A–4B | Done                        | Mock + 4 integration tests                                | OK; not hardware acceptance                                      |

This file supersedes earlier contradictory drafts that claimed “all
P0/P1 resolved.” **They are not.**

---

## 4. Roadmap to 100% wfantund parity

Ordered for maximum risk reduction. Each step should land with tests
before the next.

### Milestone A — Talk to the field stack (P0-1, P0-2, P1-9)

1. System bus by default; session bus under test feature/env.
2. Base object + `GetInterfaces` wire-compatible with C.
3. Packaging: `wfantund` / `wfanctl` names (or documented aliases).
4. **Acceptance:** `dcuctl -I wfan0 status` against `dcutund` on a
   system bus (or `dbus-run-session` with system-bus mock) without
   code hacks.

### Milestone B — Border-router data plane (P0-3, P0-4)

1. Open/configure TUN from config.
2. Address/prefix/route manager synchronized with NCP events.
3. Bidirectional IPv6 packet bridge + packet matcher.
4. **Acceptance:** with mock or hardware NCP, host pings a mesh-local /
   on-mesh address through the TUN; `IPv6:AllAddresses` matches C shape.

### Milestone C — Resilience (P0-5, P1-4, P1-5, P1-6)

1. NetworkRetain command protocol.
2. PID file, priv-drop, chroot (ordered like C).
3. Hard-reset / power path sysfs writes.
4. AutoAssociateAfterReset / AutoDeepSleep behavior.
5. **Acceptance:** NCP reset restores network without manual form when
   retain is configured; daemon runs as non-root after drop when
   configured.

### Milestone D — Full D-Bus / ops surface (P1-2, P1-3, P1-8)

1. Wire Joiner\* methods to existing task(s); add missing commissioner
   / link-metrics tasks.
2. PcapToFd / PcapTerminate + capture path.
3. PermitJoin, BeginNetWake, EnergyScanQuery, NetworkTimeUpdate.
4. **Acceptance:** method-for-method matrix vs `wpan-dbus.h` (every
   `WPANTUND_IF_CMD_*` and base cmds/signals).

### Milestone E — Observability + property parity (P1-1, P1-7)

1. StatCollector + `Stat:*` properties.
2. Close property inventory gaps for TI Wi-SUN production keys.
3. Golden tests: C vs Rust `status` / `get` string formatting.
4. **Acceptance:** README success criteria 1–2 (character-level where
   C is deterministic).

### Milestone F — Production sign-off

1. Hardware form/join with TI NCP.
2. Webapp against Rust daemon on system bus (if in scope).
3. Soak + reset/retain stress.
4. Fuzz spinel ≥ 60s; clippy/tests clean (already largely true).
5. Document intentional deviations (if any) with owner sign-off.

**Definition of done (100% wfantund parity):**

- [ ] Same D-Bus name, paths, bus type, methods, and signals as C
      (`wpan-dbus.h` + `DBusIPCAPI` + base server)
- [ ] Same config keys applied (not only parsed)
- [ ] TUN + address/route parity for border-router operation
- [ ] NetworkRetain + reset/auto-associate behavior
- [ ] Stat + Pcap when enabled in C
- [ ] Property get/set/status parity on target firmware key set
- [ ] Installable under `wfantund` / `wfanctl` names
- [ ] Hardware acceptance checklist signed off

---

## 5. C compiled-source inventory map

Quick map of production C objects → Rust **behavior** status. Use this as
a checklist of *capabilities*, not as a requirement to mirror each file
in Rust (see **Logical port** above).

### Daemon core (`src/wfantund/`)

| C source                                            | LOC (approx)         | Rust status                                                                                                        |
| --------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `wpantund.cpp`                                      | main loop, lifecycle | Partial — main/signals exist; no pid/priv/chroot                                                                   |
| `NCPInstanceBase.cpp`                               | large                | Partial — state machine / props subset                                                                             |
| `NCPInstanceBase-Addresses.cpp`                     | ~1332                | `instance/addresses.rs` — AddressManager with origin tracking, NCP table sync, user insert/remove **(done: P0-4)** |
| `NCPInstanceBase-AsyncIO.cpp`                       | ~260                 | TUN bridge tasks `tun_bridge.rs` + `TunnelIPv6Interface` in `start_pumps_impl` **(done: P0-3)**                    |
| `NCPInstanceBase-NetInterface.cpp`                  | ~477                 | TUN open/up + bidirectional stream bridge **(done: P0-3)**                                                         |
| `NCPControlInterface.cpp`                           | API surface          | Partial via D-Bus commands                                                                                         |
| `NCPInstance.cpp`                                   | factory              | `NcpInstance` wrapper                                                                                              |
| `FirmwareUpgrade.cpp`                               | ~433                 | Present + partially wired                                                                                          |
| `RunawayResetBackoffManager.cpp`                    | small                | Present (`backoff.rs`)                                                                                             |
| `NetworkRetain.cpp`                                 | ~215                 | **Missing** (P0-5)                                                                                                 |
| `Pcap.cpp`                                          | ~378                 | **Missing** (P1-2)                                                                                                 |
| `StatCollector.cpp`                                 | ~1737                | **Missing** (P1-1)                                                                                                 |
| `NCPTypes.*` / `wpan-error.*` / `wpan-properties.h` | types                | `wisun-types`                                                                                                      |
| `NCPMfgInterface_v0/v1.h`                           | mfg API              | v1 `Mfg` method present; v0 granular APIs not exposed                                                              |

### Util (compiled into daemon)

| C source                                              | Rust status                                                                                                                             |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `socket-utils.c`                                      | `dcu-serial` dispatch (UART/TCP/system) — **done**                                                                                      |
| `tunnel.c` / `TunnelIPv6Interface.*` / `netif-mgmt.c` | `dcu-tun` library — **wired into daemon** (`start_pumps_impl` opens TUN, bridge tasks use async read/write; multicast via `setsockopt`) |
| `IPv6PacketMatcher.*`                                 | `dcu-tun/packet_matcher.rs` — **not on live path** (deferred; forward-all passthrough)                                                  |
| `IPv6Helpers.*`                                       | Partial via `ipnet` / helpers                                                                                                           |
| `config-file.c`                                       | `config.rs`                                                                                                                             |
| `sec-random.c`                                        | `wisun-types/secure_random.rs`                                                                                                          |
| Timer / RingBuffer / nlpt / ValueMap / …              | std / tokio / HashMap — no port needed                                                                                                  |

### NCP Spinel plugin (`src/ncp-spinel/`)

| C source                                 | Rust status                   |
| ---------------------------------------- | ----------------------------- |
| `SpinelNCPInstance*.cpp`                 | Partial in `instance/base.rs` |
| `SpinelNCPTask*.cpp`                     | Mostly ported under `tasks/`  |
| `SpinelNCPThreadDataset.*`               | `dataset.rs` — **done**       |
| `SpinelNCPVendorCustom.*`                | No-op; optional stub OK (P2)  |
| `spinel-extra.*` / OpenThread `spinel.h` | `spinel` crate                |
| `spinel_encrypter.hpp`                   | Unused stub — P2              |

### IPC D-Bus (`src/ipc-dbus/`)

| C source            | Rust status                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------ |
| `DBusIPCAPI.cpp`    | Partial method set (P1-3)                                                                  |
| `DBUSIPCServer.cpp` | **Matched** — base `GetInterfaces` + `GetVersion` on base object, system bus (Milestone A) |
| `wpan-dbus.h`       | Constants mirrored incompletely                                                            |
| `DBUSHelpers.cpp`   | `properties::variant_to_string` subset                                                     |

### CLI (`src/wfanctl/`)

| Area                     | Status                     |
| ------------------------ | -------------------------- |
| Registered commands      | **Complete**               |
| Unregistered tool-cmd-\* | Intentionally out of scope |

---

## 6. Explicit non-goals (unchanged)

- Port `ti-wisun-webapp`
- Port `connman-plugin`
- Change Spinel wire protocol
- Change D-Bus **interface or property names** (must stay compatible)
- Port Nest-only VendorCustom property table (empty in this product)

---

## 7. Conclusion

| Question                                             | Answer                                                                                    |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Direct line-for-line C++ port?                       | **No** — **logical port**: idiomatic Rust re-implementation of external behavior.         |
| Are the phase crates scaffolded?                     | **Yes** — 1A through 4B code exists and unit/mock tests pass.                             |
| Is **wfanctl** replaceable by **dcuctl**?            | **Yes** at the registered CLI surface; **runtime** needs daemon P0-1/P0-2.                |
| Is **wfantund** replaceable by **dcutund** today?    | **No.**                                                                                   |
| What is required for **T1 field drop-in**?           | **P0-1…P0-5** (+ P1-9 packaging), measured by client/wire contracts—not by C file parity. |
| What is required for **T2 behavioral completeness**? | T1 + all P1 items, with P1-7 driven by live property inventory.                           |

Use **§4 Roadmap** as the implementation backlog. Update this file when a
P0/P1 item is closed (status + commit hash), not when a crate merely
exists on disk.

---

## 9. Implementation log (Milestones A+B — P0-1..P0-4, P1-9)

| Date       | Item                                      | Commit     | What changed                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ---------- | ----------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-07-13 | **P0-1** System bus                       | 6d1f72d    | `dcu-dbus/src/server.rs`: added `BusType` (default `System`) + `start_with_bus`; `main.rs` reads `DCU_DBUS_BUS=session` for CI. Production now claims the well-known name on the **system** bus.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-07-13 | **P0-2** Base object + `GetInterfaces`    | 6d1f72d    | New `dcu-dbus/src/base_interface.rs` registers `BaseInterface` at `WPANTUND_BASE_OBJECT_PATH` with `GetInterfaces() -> aas` (returns `[iface_name, unique_bus_name]`) and `GetVersion() -> u` (returns `2`). Removed `GetVersion` from the per-interface `WpanInterface` (C serves it from the base object). `dcuctl` now calls `GetVersion` on the base proxy and compares the numeric protocol version.                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2026-07-13 | **P1-9** Install/packaging names          | 6d1f72d    | New `packaging/install.sh` symlinks `/usr/local/sbin/wfantund -> dcutund` and `/usr/local/bin/wfanctl -> dcuctl`, installs unchanged `wpantund.conf`, and (when systemd present) `dcu-daemon.service` with `DCU_DBUS_BUS=system`. README documents the drop-in install.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 2026-07-13 | **P0-3** TUN data path                    | (staged)   | `spinel/src/property.rs`: corrected `PROP_STREAM_NET=0x72`, `PROP_STREAM_NET_INSECURE=0x73`, `PROP_STREAM_RAW=0x71`, `PROP_MAC_RAW_STREAM_ENABLED=0x37`, `PROP_IPV6_ADDRESS_TABLE=0x63` against OpenThread spec. `start_pumps_impl` opens TUN (`dcu-tun::TunnelIPv6Interface`). New `instance/tun_bridge.rs`: `ncp_to_tun` (NCP→host, from channel) and `tun_to_ncp` (host→NCP, insecure before associated). TUN brought up in `set_ncp_state` when associated. Multicast join/leave via `IPV6_JOIN_GROUP`/`IPV6_LEAVE_GROUP` in `dcu-tun/ioctl.rs`. Stream frames forwarded from `dispatch_unsolicited_static` via `stream_net_tx`.                                                                                                                                                                                                   |
| 2026-07-13 | **P0-4** Address / prefix / route manager | (staged)   | New `instance/addresses.rs`: `AddressManager` with `Origin::{Ncp, Interface, User}`, unicast/multicast/on-mesh/off-mesh maps, full-table snapshot diff methods (`apply_ncp_address_table` etc. returning `Vec<TunOp>`). NCP table props forwarded from frame-task to main loop via `address_frame_tx`/`rx`. `handle_address_frame` parses `IPV6_ADDRESS_TABLE`/`MULTICAST`/`ON_MESH_NETS`/`OFF_MESH_ROUTES`, holds write lock across TUN ops. `mirror_address_state` copies views into `DaemonState`. `address_prop_insert`/`address_prop_remove` wire `PropInsert`/`PropRemove` for `Thread:OnMeshPrefixes`/`Thread:OffMeshRoutes` with immediate AddressManager update + TUN apply. `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes` served from `properties.rs`. Added to `all_property_keys()`. |

**Verification:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` (235 tests) all pass after both milestones.

**Still open after B:** P0-5 (NetworkRetain), P1-1 (StatCollector), P1-2 (Pcap), P1-3 (13 D-Bus methods), P1-4 (lifecycle), P1-5 (GPIO), P1-6 (auto-flags), P1-7 (property inventory), P1-8 (NetworkTimeUpdate signal).

---

## 8. Re-verification log (2026-07-13)

Claims re-checked after the first draft of this rewrite:

| Claim                                        | Result                                                    | Correction applied?                           |
| -------------------------------------------- | --------------------------------------------------------- | --------------------------------------------- |
| Session vs system bus                        | **Confirmed**                                             | —                                             |
| No `GetInterfaces` / base object             | **Confirmed**                                             | —                                             |
| TUN not wired into daemon                    | **Confirmed** (`dcu_tun` only via error type + Cargo dep) | Added STREAM_NET C pointers                   |
| No address manager                           | **Confirmed**                                             | —                                             |
| No NetworkRetain runtime                     | **Confirmed**                                             | —                                             |
| Missing D-Bus methods                        | **13 registered-in-C missing**                            | Removed false gap for PermitJoin/BeginNetWake |
| ~325 vs ~40 properties                       | **Counts correct**; priority was overstated               | Split T1/T2; inventory-first for P1-7         |
| PropGet string vs variant                    | **Real wire difference**                                  | Documented as intentional deviation           |
| Doc ready as sole implementor spec for P0-3+ | **No**                                                    | Added readiness table + per-gap design note   |

**Honest answer to “is this ready for the implementor?”**

- **Yes** for prioritization, ownership of gaps, and starting **Milestone A**
  (P0-1, P0-2, P1-9).
- **Partially** for Milestone B+ (TUN/addresses/retain): correct *that*
  the work is needed and *where* in C to look; still needs a short design
  pass before coding.
- **Do not** treat “100% of C” as “implement 325 properties and every
  Thread link-metrics path on day one” without product prioritization.
