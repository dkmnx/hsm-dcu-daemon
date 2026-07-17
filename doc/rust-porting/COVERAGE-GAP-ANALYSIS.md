# Rust Porting Coverage Gap Analysis

**Date:** 2026-07-15 (Milestones C + D closures verified against committed code)  
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
| Role      | C (autotools)              | Rust (Cargo)                        |
| --------- | -------------------------- | ----------------------------------- |
| Daemon    | `sbin_PROGRAMS = wfantund` | `[[bin]] name = "dcutund"`          |
| CLI       | `bin_PROGRAMS = wfanctl`   | package `dcuctl`                    |
| Interface | `wfan0`                    | `wfan0` (default)                   |
| Config    | `/etc/wpantund.conf`       | `/etc/wpantund.conf` (default `-c`) |
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
hardening). Do not treat “321 property defines” as a blind implementor
checklist — many are Thread-era; inventory against live C `status` /
firmware first (P1-7).

### Implementor readiness of *this document*

| Ready now?                      | What this doc is                                                      | What it is not                                                                                          |
| ------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Yes as backlog / triage**     | Verified gap list, priorities, C file pointers, acceptance milestones | —                                                                                                       |
| **Not alone as design tickets** | —                                                                     | Per-gap wire-format specs, zbus signatures, TID state-machine detail, effort estimates, golden fixtures |
| **Not a C→Rust file map**       | C paths cite *behavior to re-implement*                               | A mandate to recreate C module layout                                                                   |

**Before coding each P0/P1 item**, write a short implementor note in this
document with: external contract (D-Bus/Spinel/
config), C entry points as reference, idiomatic Rust design, files to
touch, tests. Milestone A (P0-1/P0-2) is small enough to implement from
this doc + `DBUSIPCServer.cpp` alone.

---

## Executive summary

| Target                 | Drop-in today?                   | Verdict                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **wfanctl → dcuctl**   | **Yes** (CLI surface)            | All 9 registered C commands ported. Same D-Bus client contract. Rename/package only (see P1-9). **Runtime unblocked after P0-1/P0-2 closed** — now works against a `dcutund` on the system bus.                                                                                                                                                                                                                                                                                               |
| **wfantund → dcutund** | **Partial** (Milestones A+B+C+D) | Core Spinel/D-Bus/task stack exists and mock-e2e passes. System bus, base object, TUN data path, address/prefix/route manager, NetworkRetain, Pcap, lifecycle (pid/chroot/privdrop), GPIO, AutoAssociateAfterReset, full 45/45 D-Bus method surface, and the `NetworkTimeUpdate` signal. Remaining gaps: property inventory (~40 registered handlers vs 321 C defines, P1-7) and hardware acceptance (Milestone F). |

### Headline blockers for T1 field drop-in (P0)

| ID       | Gap                                       | Evidence (re-verified 2026-07-14)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Status |
| -------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| **P0-1** | **System D-Bus bus**                      | C: `DBUSIPCServer.cpp:68` `DBUS_BUS_SYSTEM`. Rust: `dcu-dbus/src/server.rs` `Connection::system()` via `BusType::System` (default) + `DCU_DBUS_BUS` env override; `start_with_bus` added. **Closed** (6d1f72d)                                                                                                                                                                                                                                                                                                                                                                                                      | DONE   |
| **P0-2** | **Base object `GetInterfaces`**           | C: `DBUSIPCServer.cpp` ~286– registers base path method. Rust: `base_interface.rs` registers `BaseInterface` at `WPANTUND_BASE_OBJECT_PATH` with `GetInterfaces` (`aas`) + `GetVersion` (`u`); per-iface `GetVersion` removed from `interface.rs`. `dcuctl` calls `GetVersion` on base proxy. **Closed** (6d1f72d)                                                                                                                                                                                                                                                                                                  | DONE   |
| **P0-3** | **TUN data path**                         | C: `SpinelNCPInstance.cpp` ~6617+ `SPINEL_PROP_STREAM_NET`. Rust: `start_pumps_impl` opens TUN (`dcu-tun::TunnelIPv6Interface`) + spawns `ncp_to_tun`/`tun_to_ncp` bridge tasks; `set_ncp_state` brings TUN up on interface-up. `crates/instance/tun_bridge.rs`. Added multicast join/leave (ioctl + interface). Spinel constants `PROP_STREAM_NET`/`_INSECURE`/`_RAW`/`_DEBUG` corrected against spec. **Closed**                                                                                                                                                                                                  | DONE   |
| **P0-4** | **IPv6 address / prefix / route manager** | C: `NCPInstanceBase-Addresses.cpp` (~1332 LOC). Rust: `crates/instance/addresses.rs` — `AddressManager` with origin tracking (NCP/Interface/User), `apply_ncp_address_table`/`multicast`/`on_mesh`/`off_mesh` methods diffing full-table snapshots and returning `TunOp` vecs. NCP table frames forwarded from frame-task to main loop via channel; `apply_tun_ops` under write lock. `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes` served from DaemonState mirror. `PropInsert`/`PropRemove` for prefix/route keys updates AddressManager immediately + NCP push. **Closed** | DONE   |
| **P0-5** | **NetworkRetain**                         | C `NetworkRetain.cpp` (~215 LOC). Rust: `network_retain.rs` — `RetainAction::Recall`/`Erase` (Save branch removed — inert under TI_WISUN_FAN `has_joined()=true`). `handle_state_change()` returns `JoinHandle`, awaited before AutoAssociate. Command spawned via `tokio::process::Command` with fixed arg (S/R/E). Logged at `error` level on failure. Config key `Config:Daemon:NetworkRetainCommand`. **Closed** (commit c8f0a10)                                                                                                                                                                               | DONE   |

### Secondary gaps (P1) — T2 / ops completeness

| ID        | Gap                                       | C source                                                                                    | Notes                                                                                                                                                    |
| --------- | ----------------------------------------- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **P1-1**  | StatCollector + `Stat:*`                  | `StatCollector.cpp` (~1.7k)                                                                 | **Closed**. `instance/stat_collector.rs` — packet/NCP-state recording, 5 formatted properties, 11 unit tests.                                            |
| **P1-2**  | Pcap + `PcapToFd` / `PcapTerminate`       | `Pcap.cpp` + registered in `DBusIPCAPI`                                                     | **Closed** (commit c8f0a10). `pcap.rs` + `AtomicBool` gate + `spawn_blocking` writes.                                                                    |
| **P1-3**  | Missing **registered** D-Bus methods      | See §2.3 method matrix                                                                      | **Closed** (commits dcd085d + 6561ef4). **45/45 registered AND implemented** — all route to real task handlers; no stubs remain.                         |
| **P1-4**  | PID file, priv-drop, chroot               | `wpantund.cpp`                                                                              | **Closed** (commit c8f0a10). `lifecycle.rs`: PID via `unlinkat(dirfd)`, `getpwnam_r`, `setgroups` before `setgid`.                                       |
| **P1-5**  | Hard-reset / power GPIO paths             | config keys                                                                                 | **Closed** (commit c8f0a10). `ncp_gpio::hard_reset()` wired into reset path (`base.rs:1513`).                                                            |
| **P1-6**  | AutoAssociateAfterReset                   | config flag                                                                                 | **Closed** (commit c8f0a10). Sends `PROP_NET_STACK_UP=1` on `Initializing→Offline` (`base.rs:1386`).                                                     |
| **P1-7**  | Property surface vs production set        | `wpan-properties.h` (**321** defines) vs **~40** Rust registered handlers + 126 key strings | **Open.** Inventory against live TI firmware required before expanding.                                                                                  |
| **P1-8**  | `NetworkTimeUpdate` signal                | C connects `mOnNetworkTimeUpdate`                                                           | **Closed** (commit 6561ef4). `signals::emit_network_time_update` wired in `main.rs`; `base.rs` decodes `PROP_THREAD_NETWORK_TIME` into the emit channel. |
| **P1-9**  | Binary / packaging names                  | Makefile vs Cargo                                                                           | **Closed** (6d1f72d). Symlink install script present.                                                                                                    |
| **P1-10** | Minor config gaps + dcu-serial transports | See §2.3 config gap table                                                                   | **Partial.** CCA threshold + TX power + TerminateOnFault + system-socketpair closed; 2 config keys + fd: remain.                                         |

> **Note:** All transports are now implemented: `system:` (PTY), `system-forkpty:` (PTY),
> `system-socketpair:` (socketpair), and `fd:` (raw descriptor). See `dcu-serial/src/system.rs`.
> No transport stubs remain.

### Known intentional / already-documented deviations (not “missing code”)

| Item                  | Reality                                                                                                                 | Implementor note                                                                                                                      |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `PropGet` return type | C returns typed **variant** via callback helper; Rust returns **stringified** `String` (`interface.rs` + phase-2A note) | Clients that only print strings (dcuctl, much of webapp PropGet) OK; typed-variant clients need audit                                 |
| `Status` dict values  | Rust `HashMap<String, String>`; C builds variant dict                                                                   | Same stringification caveat                                                                                                           |
| Joiner task file      | `tasks/joiner_commission.rs` exists; `JoinerAttach`/`Start`/`Stop`/`Commissioning` wired as D-Bus methods               | `JoinerAttach` uses plain `NET_STACK_UP=true` (no PSKd); `JoinerAdd`/`JoinerRemove` now route to `commissioner_ops.rs` (P1-3 closed). |

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

| Area                               | Rust location                  | Notes                                                                                                                                 |
| ---------------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| Types / property key constants     | `crates/wisun-types`           | Includes secure RNG, dataset-related keys                                                                                             |
| Spinel codec + HDLC                | `crates/spinel`                | Fuzz target present                                                                                                                   |
| TUN library                        | `crates/dcu-tun`               | Device, ioctl, packet matcher, multicast — **wired into daemon**                                                                      |
| TUN ↔ NCP bridge                   | `instance/tun_bridge.rs`       | `ncp_to_tun` + `tun_to_ncp` async tasks (STREAM_NET/INSECURE)                                                                         |
| Address / prefix / route manager   | `instance/addresses.rs`        | `AddressManager` with origin tracking, NCP table sync, user insert/remove                                                             |
| Multicast join/leave               | `dcu-tun/ioctl.rs`             | `IPV6_JOIN_GROUP`/`IPV6_LEAVE_GROUP` via `setsockopt`                                                                                 |
| Serial / TCP / `system:` / PTY     | `crates/dcu-serial`            | Transport dispatch implemented                                                                                                        |
| D-Bus interface methods (subset)   | `crates/dcu-dbus`              | Form/Join/Leave/Status/Prop\*/scans/Mfg/… + **IPv6:AllAddresses, IPv6:Routes, Thread:OnMeshPrefixes, Thread:OffMeshRoutes**           |
| D-Bus signals (subset)             | `crates/dcu-dbus/signals.rs`   | NetScanBeacon, EnergyScanResult, PropChanged, InterfaceAdded/Removed                                                                  |
| Daemon core + Spinel I/O           | `crates/dcu-tunnel-daemon`     | Response table, io_task, command dispatch                                                                                             |
| Spinel tasks                       | `tasks/*`                      | form, join, leave, scan, sleep, peek, topology, joiner_commission, …                                                                  |
| Operational dataset                | `dataset.rs`                   | Codec + DaemonState mirror for `Dataset:*`                                                                                            |
| Firmware upgrade helpers           | `firmware_upgrade.rs`          | Wired when `AutoFirmwareUpdate` is set                                                                                                |
| Config parser                      | `config.rs`                    | wpantund.conf key subset                                                                                                              |
| Mock NCP + e2e                     | `dcu-mock` + integration tests | form/join/startup/timeout vs mock                                                                                                     |
| Runaway reset backoff              | `tasks/backoff.rs`             | Present                                                                                                                               |
| Daemon lifecycle (PID/chroot/drop) | `instance/lifecycle.rs`        | PID file via `unlinkat(dirfd)`, chroot, `getpwnam_r` + `setgroups` priv-drop. Applied after pumps in `main.rs`. **(done: P0-5/P1-4)** |
| NCP GPIO reset / power toggle      | `instance/ncp_gpio.rs`         | `hard_reset()` wired into reset path; `power_toggle()` available. sysfs write. **(done: P1-5)**                                       |
| NetworkRetain                      | `instance/network_retain.rs`   | `Recall`/`Erase` on NCP state transitions; awaited before AutoAssociate. Spawns external command. **(done: P0-5)**                    |
| Pcap capture                       | `instance/pcap.rs`             | `PcapManager` with `AtomicBool` gate + `spawn_blocking` writes; `PcapToFd`/`PcapTerminate` wired. **(done: P1-2)**                    |
| AutoAssociateAfterReset            | `instance/base.rs:1386`        | Sends `PROP_NET_STACK_UP=1` on `Initializing→Offline` when flag set. **(done: P1-6)**                                                 |

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

Rust: `network_retain.rs` — `NetworkRetain` struct with `handle_state_change(old, new)`
returning `Option<JoinHandle<()>>`. Called from `set_ncp_state` in `base.rs`.
Recall/erase spawned via `tokio::process::Command::new(&cmd).arg(&arg)`.
`RetainAction::Save` branch removed (inert under TI_WISUN_FAN — `has_joined()` always
returns `true`). Failures logged at `error` level (previously `warn`).

**Status:** CLOSED (commit c8f0a10). **Caveat:** retain helper runs after
chroot + priv-drop; command path must exist inside the chroot and be runnable
by the dropped uid.

**Implementation:**

| Capability                        | Rust location                                                                                                            |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Action determination              | `network_retain.rs::action_for_transition(old, new)` — Recall on `Initializing→Offline`, Erase on `Commissioned→Offline` |
| Command spawn                     | `tokio::process::Command::new(cmd).arg(arg).output().await` inside `tokio::spawn`                                        |
| Await before AutoAssociate        | `base.rs:1385` — `if let Some(h) = handle { h.await; }` before the `AutoAssociateAfterReset` block                       |
| Post-chroot behavior              | Documented: command must be inside chroot, runnable by dropped uid; failures at `error` level                            |

---

#### P1-1 — StatCollector

C exposes many `Stat:*` properties (node TX/RX history, link quality,
NCP state history, …) via `StatCollector::is_a_stat_property`.

**Status:** CLOSED. `instance/stat_collector.rs` — `StatCollector` with
10 counters, 3 bounded histories (VecDeque max 64), IPv6 header parsing
for protocol classification (UDP/TCP/ICMP), and 5 format methods
(`stat_rx`, `stat_tx`, `stat_ncp`, `stat_short`, `stat_long`). Wired via
`Arc<RwLock<StatCollector>>` into bridge tasks (packet recording),
`set_ncp_state` (state recording), and `Command::GetProperty` (computed
property serving). 11 unit tests. `cargo fmt`, `clippy`, `cargo test
--workspace` all pass.

#### P1-2 — Pcap

C: `PcapManager` + D-Bus `PcapToFd` / `PcapTerminate`; Spinel path
pushes frames when pcap is active.

**Status:** CLOSED (commit c8f0a10).

| Capability                   | Rust location                                                    | Notes                                                               |
| ---------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------- |
| `PcapToFd` D-Bus method      | `interface.rs` → `Command::PcapToFd` → `pcap.insert_fd(fd)`      | `fd` transferred via `mem::forget` (ownership handoff before drop)  |
| `PcapTerminate` D-Bus method | `interface.rs` → `Command::PcapTerminate` → `pcap.terminate()`   | Closes all capture FDs via `libc::close`                            |
| Hot-path gate                | `pcap.rs::is_enabled()` → `AtomicBool::load(Relaxed)`            | O(1), no async lock on the frame-processing task                    |
| Frame capture                | `pcap.rs::push_packet(Vec<u8>)` via `spawn_blocking`             | Writes pcap records off the async worker; broken FDs auto-removed   |
| Capture header               | `pcap.rs::write_pcap_header` — DLT_PPI, snaplen 256KB            | Written to each FD on `insert_fd`                                   |

**Caveat:** `pcap.rs` uses `#[allow(unsafe_code)]` for `from_raw_fd`/`close`
(4 unsafe blocks). File-descriptor ownership is manual (caller-owned FDs).

#### P1-3 — D-Bus interface methods (registered in C only)

**Method matrix (re-verified 2026-07-14):**

- C header defines: **47** `WPANTUND_IF_CMD_*`
- C actually registered (`INTERFACE_CALLBACK_CONNECT`): **45**
  (excludes `PermitJoin`, `BeginNetWake` — header-only)
- Rust `#[zbus(name)]` on iface: **45** (all C registered methods now present)
  Includes `GetVersion` (C serves from the base server as `WPANTUND_IF_GET_VERSION`)

**All 45 methods are registered and implemented** — no missing D-Bus surface and
**no remaining stubs**. Each method dispatches to a real task handler in
`tasks/commissioner_ops.rs`, `tasks/joiner_commission.rs`, etc.:

| Method                   | Status                               | Notes                                                                     |
| ------------------------ | ------------------------------------ | ------------------------------------------------------------------------- |
| `PcapToFd`               | **Done** (P1-2)                      | FD ownership transferred via `mem::forget`                                |
| `PcapTerminate`          | **Done** (P1-2)                      | Closes all capture FDs                                                    |
| `JoinerAttach`           | **Done** (plain attach)              | `PROP_NET_STACK_UP=true`, no PSKd (matches C `SpinelNCPTaskJoinerAttach`) |
| `JoinerStart`            | **Done** (commissioning)             | Wired to `joiner_commission(action=true)`                                 |
| `JoinerStop`             | **Done** (commissioning)             | Wired to `joiner_commission(action=false)`                                |
| `JoinerCommissioning`    | **Done** (deprecated alias)          | Same as `JoinerStart`                                                     |
| `JoinerAdd`              | **Done** (6561ef4)                   | `commissioner_ops::joiner_add` → Spinel commissioner joiner-add           |
| `JoinerRemove`           | **Done** (6561ef4)                   | `commissioner_ops::joiner_remove` → Spinel commissioner joiner-remove     |
| `LinkMetricsQuery`       | **Done** (6561ef4)                   | `commissioner_ops::link_metrics_query` → Spinel link-metrics query        |
| `LinkMetricsProbe`       | **Done** (6561ef4)                   | Spinel link-metrics task (commissioner_ops)                               |
| `LinkMetricsMgmtForward` | **Done** (6561ef4)                   | Spinel link-metrics task (commissioner_ops)                               |
| `LinkMetricsMgmtEnhAck`  | **Done** (6561ef4)                   | Spinel link-metrics task (commissioner_ops)                               |
| `EnergyScanQuery`        | **Done** (6561ef4)                   | `commissioner_ops::energy_scan_query` → Spinel energy-scan query          |

**Reproduce the matrix (run before claiming a method is done):**

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
> `NETWORK_WAKE_BEGIN`), Rust now implements **45** (`/tmp/r.txt`). No
> missing methods remain and **no stubs remain** — all 45 route to real task handlers
> (see table above).

**Registered in C, now in Rust (45):** All C registered methods present.

**No stubs remain:** all 45 methods (including JoinerAdd, JoinerRemove, the four
LinkMetrics methods, and EnergyScanQuery) route to real `Command` handlers implemented
in `crates/dcu-tunnel-daemon/src/tasks/commissioner_ops.rs`.

**Not a gap (C also does not register):** `PermitJoin`, `BeginNetWake`.

**Base path (P0-2, DONE):** `GetInterfaces` served from base object (`base_interface.rs`).

#### P1-4 — Daemon process lifecycle

**Status:** CLOSED (commit c8f0a10). `lifecycle.rs` applies
PID file → chroot → priv-drop after `start_pumps()` in `main.rs`.

| Config key                     | C behavior                | Rust status                                                                    |
| ------------------------------ | ------------------------- | ------------------------------------------------------------------------------ |
| `Config:Daemon:PIDFile`        | write PID, unlink on exit | `write_pidfile()` → `PidFileGuard` with `unlinkat(dirfd)` (survives chroot)    |
| `Config:Daemon:PrivDropToUser` | setgid/setuid after bind  | `priv_drop_to()` → `getpwnam_r` + `setgroups(0, NULL)` + `setgid` + `setuid`   |
| `Config:Daemon:Chroot`         | chroot + chdir            | `chroot_to()` → `chdir` → `chroot` → `chdir("/")`                              |
| syslog / log mask              | syslog                    | tracing only (acceptable if journald-compatible; documented)                   |

**Caveat:** PID file cleanup uses `unlinkat` anchored to a parent-directory FD
opened before chroot. `Dir*` from `opendir` is intentionally leaked (one-time,
acceptable for a daemon).

#### P1-5 — Hard reset / power GPIO

**Status:** CLOSED (commit c8f0a10). `ncp_gpio::hard_reset()` writes
`'0'` → sleep 20ms → `'1'` to the configured `HardResetPath` sysfs file.
Wired into the reset path in `base.rs:1510` before `CMD_RESET`.

`power_toggle()` is available but not wired into a production path (available
for future use).

#### P1-6 — AutoAssociateAfterReset

**Status:** CLOSED (commit c8f0a10). When `daemon_auto_associate_after_reset`
is true (default) and the NCP transitions from `Initializing → Offline`,
sends `PROP_NET_STACK_UP=1` (`base.rs:1396`).

(AutoDeepSleep is now implemented — tickle timer on DeepSleep entry)
AutoAssociate; deferred).

#### P1-7 — Property surface

|                        | C                                                                                                        | Rust                                                                                                                                                                     |
| ---------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Named property defines | **321** in `wpan-properties.h` (verified 2026-07-14)                                                     | **126** key strings in `wisun-types/property_key.rs` + **~40** registered handlers in `instance/property_handlers.rs`                                                    |
| Spinel handler map     | large switch tables in SpinelNCPInstance                                                                 | ~40 entries in `property_handlers.rs` (NCP-forwarded) + daemon-local keys in `base.rs` (IPv6:AllAddresses, IPv6:Routes, Thread:OnMeshPrefixes, OffMeshRoutes, Dataset:*) |

**Important:** Not every C key is active on TI Wi-SUN firmware. The 321
includes Thread-era properties not used by TI. Parity requires:

1. **Inventory keys actually returned by C `Status`/`PropGet` on target firmware.**
2. Implement daemon-local vs NCP-forwarded handlers for that set.
3. Golden-test string formatting against C (`variant_to_string` parity).

**Current status:** ~40 registered handlers cover the core TI Wi-SUN
NCP/Network/PHY/MAC-timing properties. Unknown keys fail with
`"unknown property"` (not passthrough). The gap is real but narrower than
321→40 once Thread-only keys are excluded.

#### P1-8 — `NetworkTimeUpdate` signal

**Status:** CLOSED (commit 6561ef4). `dcu-dbus/src/signals.rs` implements
`emit_network_time_update`; `main.rs` spawns the receiver task that emits the
signal, and `instance/base.rs` decodes unsolicited `PROP_THREAD_NETWORK_TIME`
frames into the emit channel.

#### P1-9 — Install names

For drop-in packages:

- Install `dcutund` as `/usr/sbin/wfantund` (or symlink)
- Install `dcuctl` as `/usr/bin/wfanctl` (or symlink)
- Ship `wpantund.conf` / unit files unchanged where possible

### 2.3 Minor config gaps (T2 behavioral completeness)

Several `wpantund.conf` keys are parsed into the `Config` struct but
never read at runtime. They don’t block T1 field drop-in but represent
genuine behavioral differences from the C daemon.

| Config key                          | Field                             | C behavior                                     | Rust status                                               |
| ----------------------------------- | --------------------------------- | ---------------------------------------------- | --------------------------------------------------------- |
| `Config:Daemon:AutoDeepSleep`       | `daemon_auto_deep_sleep`          | NCP deep-sleep tickle timer (4200 s)           | **Closed.** Tickle resets NCP after 70 min in deep sleep. |
| `Config:Daemon:TerminateOnFault`    | `daemon_terminate_on_fault`       | Exit daemon on NCP `FAULT` state               | **Closed.** Exits when NCP enters FAULT and flag is set.  |
| `Config:Daemon:SyslogMask`          | `daemon_syslog_mask`              | syslog priority mask                           | Parsed only; Rust uses `tracing` (acceptable)             |
| `Config:NCP:CCATreshold`            | `nc_cca_threshold`                | `PROP_PHY_CCA_THRESHOLD` sent to NCP           | **Closed.** Sent on `Initializing→Offline` (`base.rs`).   |
| `Config:NCP:TXPower`                | `nc_tx_power`                     | `PROP_PHY_TX_POWER` sent to NCP                | **Closed.** Sent on `Initializing→Offline` (`base.rs`).   |
| `Config:IPv6:WPANTundGlobalAddress` | `ipv6_wfantund_global_address`    | Global address on TUN interface                | Parsed only; C behavior not investigated                  |

#### dcu-serial transport status

The `dcu-serial` crate supports all transport types: UART, TCP, `system:`
(PTY), `system-forkpty:` (PTY), `system-socketpair:` (socketpair), and
`fd:` (raw descriptor). No transport stubs remain.

All four `system:*` and `fd:` implementations live in `dcu-serial/src/system.rs`.

---

## 3. Phase docs vs code (staleness)

| Phase | README status               | Code                                                      | Doc accuracy note                                                |
| ----- | --------------------------- | --------------------------------------------------------- | ---------------------------------------------------------------- |
| 1A–2B | Done                        | Present                                                   | OK                                                               |
| 3A    | Done                        | Core + lifecycle + retain + pcap + GPIO + auto-associate  | All phase-3A items now closed (uncommitted diff)                 |
| 3B    | Done                        | Tasks present                                             | OK                                                               |
| 3C    | Done                        | `dataset.rs` present                                      | OK                                                               |
| 4A–4B | Done                        | Mock + 4 integration tests                                | OK; not hardware acceptance                                      |

This file supersedes earlier contradictory drafts that claimed “all
P0/P1 resolved.” **Milestones A+B+C+D are all done; only P1-1 (StatCollector)
and P1-7 (property inventory) plus hardware acceptance (Milestone F) remain.**

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

### Milestone C — Resilience (P0-5, P1-4, P1-5, P1-6) **DONE**

1. ~~NetworkRetain command protocol.~~ ✅
2. ~~PID file, priv-drop, chroot (ordered like C).~~ ✅
3. ~~Hard-reset / power path sysfs writes.~~ ✅
4. ~~AutoAssociateAfterReset behavior.~~ ✅
5. **Acceptance:** NCP reset restores network without manual form when
   retain is configured; daemon runs as non-root after drop when
   configured. **Pending hardware test.**

### Milestone D — Full D-Bus / ops surface (P1-2, P1-3, P1-8) **DONE**

1. ~~Wire Joiner\* methods to existing task(s).~~ ✅ (`JoinerAttach` = plain attach;
   `JoinerStart`/`Stop`/`Commissioning` = commissioning task; `JoinerAdd`/`JoinerRemove` =
   commissioner ops; all 4 LinkMetrics + `EnergyScanQuery` = `commissioner_ops` tasks).
2. ~~PcapToFd / PcapTerminate + capture path.~~ ✅
3. ~~LinkMetrics tasks and `EnergyScanQuery`.~~ ✅ (real Spinel task handlers, no stubs).
4. ~~`NetworkTimeUpdate` signal.~~ ✅ (commit 6561ef4).
5. **Acceptance:** method-for-method matrix vs `wpan-dbus.h` — **45/45 registered AND implemented**.

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

- [x] Same D-Bus name, paths, bus type, methods, and signals as C
      (`wpan-dbus.h` + `DBusIPCAPI` + base server) — **45/45 methods registered**
- [ ] Same config keys applied (not only parsed) — **P1-7 partial**
- [x] TUN + address/route parity for border-router operation
- [x] NetworkRetain + reset/auto-associate behavior
- [ ] Stat + Pcap when enabled in C — **Pcap done, StatCollector missing**
- [ ] Property get/set/status parity on target firmware key set — **~40/321 handlers**
- [ ] Installable under `wfantund` / `wfanctl` names — **P1-9 done but checkbox kept for packaging verification**
- [ ] Hardware acceptance checklist signed off

---

## 5. C compiled-source inventory map

Quick map of production C objects → Rust **behavior** status. Use this as
a checklist of *capabilities*, not as a requirement to mirror each file
in Rust (see **Logical port** above).

### Daemon core (`src/wfantund/`)

| C source                                            | LOC (approx)         | Rust status                                                                                                                                |
| --------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `wpantund.cpp`                                      | main loop, lifecycle | **Done** — lifecycle applied in `main.rs` after pumps (commit c8f0a10); PID/chroot/priv-drop in `lifecycle.rs`                             |
| `NCPInstanceBase.cpp`                               | large                | Partial — state machine / props subset                                                                                                     |
| `NCPInstanceBase-Addresses.cpp`                     | ~1332                | `instance/addresses.rs` — AddressManager with origin tracking, NCP table sync, user insert/remove **(done: P0-4)**                         |
| `NCPInstanceBase-AsyncIO.cpp`                       | ~260                 | TUN bridge tasks `tun_bridge.rs` + `TunnelIPv6Interface` in `start_pumps_impl` **(done: P0-3)**                                            |
| `NCPInstanceBase-NetInterface.cpp`                  | ~477                 | TUN open/up + bidirectional stream bridge **(done: P0-3)**                                                                                 |
| `NCPControlInterface.cpp`                           | API surface          | Partial via D-Bus commands                                                                                                                 |
| `NCPInstance.cpp`                                   | factory              | `NcpInstance` wrapper                                                                                                                      |
| `FirmwareUpgrade.cpp`                               | ~433                 | Present + partially wired                                                                                                                  |
| `RunawayResetBackoffManager.cpp`                    | small                | Present (`backoff.rs`)                                                                                                                     |
| `NetworkRetain.cpp`                                 | ~215                 | `network_retain.rs` — `Recall`/`Erase` on state transitions; `Save` branch removed (inert). Awaited before AutoAssociate. **(done: P0-5)** |
| `Pcap.cpp`                                          | ~378                 | `pcap.rs` — `AtomicBool` gate + `spawn_blocking` writes; `PcapToFd`/`PcapTerminate` D-Bus methods wired. **(done: P1-2)**                  |
| `StatCollector.cpp`                                 | ~1737                | `stat_collector.rs` — packet/NCP-state recording, 5 format methods, 11 unit tests. **(done: P1-1)**                                        |
| `NCPTypes.*` / `wpan-error.*` / `wpan-properties.h` | types                | `wisun-types`                                                                                                                              |
| `NCPMfgInterface_v0/v1.h`                           | mfg API              | v1 `Mfg` method present; v0 granular APIs not exposed                                                                                      |

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

| C source            | Rust status                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------------ |
| `DBusIPCAPI.cpp`    | **45/45 methods registered AND implemented** — no stubs remain (commits dcd085d + 6561ef4)                   |
| `DBUSIPCServer.cpp` | **Matched** — base `GetInterfaces` + `GetVersion` on base object, system bus (Milestone A)                   |
| `wpan-dbus.h`       | Constants mirrored incompletely                                                                              |
| `DBUSHelpers.cpp`   | `properties::variant_to_string` subset                                                                       |

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

| Question                                             | Answer                                                                                                                                                                                 |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Direct line-for-line C++ port?                       | **No** — **logical port**: idiomatic Rust re-implementation of external behavior.                                                                                                      |
| Are the phase crates scaffolded?                     | **Yes** — 1A through 4B code exists and unit/mock tests pass.                                                                                                                          |
| Is **wfanctl** replaceable by **dcuctl**?            | **Yes** at the registered CLI surface; **runtime** needs daemon P0-1/P0-2.                                                                                                             |
| Is **wfantund** replaceable by **dcutund** today?    | **Partial.** T1 data plane + lifecycle + full D-Bus surface + `NetworkTimeUpdate` done; remaining: StatCollector (P1-1), property inventory (P1-7), hardware acceptance (Milestone F). |
| What is required for **T1 field drop-in**?           | **P0-1…P0-5** ✅ + **P1-9** ✅. Data plane + lifecycle done. Remaining: property handler coverage (P1-7) for client parity.                                                              |
| What is required for **T2 behavioral completeness**? | T1 + P1-1 (StatCollector), P1-7 (property inventory), P1-10 (config gaps + dcu-serial transports), Milestone F (hardware). (P1-3 + P1-8 now closed.)                                   |

Use **§4 Roadmap** as the implementation backlog. Update this file when a
P0/P1 item is closed (status + commit hash), not when a crate merely
exists on disk.

---

## 8. Implementation log (Milestones A+B+C — P0-1..P0-5, P1-2..P1-6, P1-9)

| Date       | Item                                          | Commit     | What changed                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ---------- | --------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-07-13 | **P0-1** System bus                           | 6d1f72d    | `dcu-dbus/src/server.rs`: added `BusType` (default `System`) + `start_with_bus`; `main.rs` reads `DCU_DBUS_BUS=session` for CI. Production now claims the well-known name on the **system** bus.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-07-13 | **P0-2** Base object + `GetInterfaces`        | 6d1f72d    | New `dcu-dbus/src/base_interface.rs` registers `BaseInterface` at `WPANTUND_BASE_OBJECT_PATH` with `GetInterfaces() -> aas` (returns `[iface_name, unique_bus_name]`) and `GetVersion() -> u` (returns `2`). Removed `GetVersion` from the per-interface `WpanInterface` (C serves it from the base object). `dcuctl` now calls `GetVersion` on the base proxy and compares the numeric protocol version.                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2026-07-13 | **P1-9** Install/packaging names              | 6d1f72d    | New `packaging/install.sh` symlinks `/usr/local/sbin/wfantund -> dcutund` and `/usr/local/bin/wfanctl -> dcuctl`, installs unchanged `wpantund.conf`, and (when systemd present) `dcu-daemon.service` with `DCU_DBUS_BUS=system`. README documents the drop-in install.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 2026-07-13 | **P0-3** TUN data path                        | c8f0a10    | `spinel/src/property.rs`: corrected `PROP_STREAM_NET=0x72`, `PROP_STREAM_NET_INSECURE=0x73`, `PROP_STREAM_RAW=0x71`, `PROP_MAC_RAW_STREAM_ENABLED=0x37`, `PROP_IPV6_ADDRESS_TABLE=0x63` against OpenThread spec. `start_pumps_impl` opens TUN (`dcu-tun::TunnelIPv6Interface`). New `instance/tun_bridge.rs`: `ncp_to_tun` (NCP→host, from channel) and `tun_to_ncp` (host→NCP, insecure before associated). TUN brought up in `set_ncp_state` when associated. Multicast join/leave via `IPV6_JOIN_GROUP`/`IPV6_LEAVE_GROUP` in `dcu-tun/ioctl.rs`. Stream frames forwarded from `dispatch_unsolicited_static` via `stream_net_tx`.                                                                                                                                                                                                   |
| 2026-07-13 | **P0-4** Address / prefix / route manager     | c8f0a10    | New `instance/addresses.rs`: `AddressManager` with `Origin::{Ncp, Interface, User}`, unicast/multicast/on-mesh/off-mesh maps, full-table snapshot diff methods (`apply_ncp_address_table` etc. returning `Vec<TunOp>`). NCP table props forwarded from frame-task to main loop via `address_frame_tx`/`rx`. `handle_address_frame` parses `IPV6_ADDRESS_TABLE`/`MULTICAST`/`ON_MESH_NETS`/`OFF_MESH_ROUTES`, holds write lock across TUN ops. `mirror_address_state` copies views into `DaemonState`. `address_prop_insert`/`address_prop_remove` wire `PropInsert`/`PropRemove` for `Thread:OnMeshPrefixes`/`Thread:OffMeshRoutes` with immediate AddressManager update + TUN apply. `IPv6:AllAddresses`, `IPv6:Routes`, `Thread:OnMeshPrefixes`, `Thread:OffMeshRoutes` served from `properties.rs`. Added to `all_property_keys()`. |
| 2026-07-14 | **P1-3** 13 missing D-Bus methods             | dcd085d    | `interface.rs` + `base.rs` command dispatch for JoinerAdd/Remove, 4×LinkMetrics, EnergyScanQuery, and others; 45/45 methods registered.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 2026-07-14 | **Milestone C** resilience (P0-5, P1-2..P1-6) | c8f0a10    | Committed the previously-staged Milestone B work (TUN/addresses) plus lifecycle (PID/chroot/priv-drop), NetworkRetain, Pcap, GPIO reset, AutoAssociateAfterReset.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 2026-07-15 | **Milestone D** 7 D-Bus methods + signal      | 6561ef4    | Implemented `JoinerAdd`/`JoinerRemove`, 4×`LinkMetrics*`, `EnergyScanQuery` as real task handlers in `tasks/commissioner_ops.rs`; added `NetworkTimeUpdate` signal (`signals::emit_network_time_update`, wired in `main.rs` + `base.rs`). No D-Bus stubs remain.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

**Verification:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` (235 tests) all pass after both milestones.

**Still open (post-D):** P1-1 (StatCollector), P1-7 (property inventory — ~40/321 handlers), Milestone F (hardware acceptance).

---

## 9. Re-verification log

### 2026-07-14 (Milestone C closure)

Claims re-checked after Milestone C closure (uncommitted diff at the time):

| Claim                                        | Result                                                                                                   | Correction applied?                                    |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Session vs system bus                        | **Confirmed**                                                                                            | —                                                      |
| No `GetInterfaces` / base object             | **Confirmed**                                                                                            | —                                                      |
| TUN not wired into daemon                    | **Confirmed** (`dcu_tun` only via error type + Cargo dep)                                                | Added STREAM_NET C pointers                            |
| No address manager                           | **Confirmed**                                                                                            | —                                                      |
| No NetworkRetain runtime                     | **Closed** — `network_retain.rs` now handles Recall/Erase                                                | Updated §2.2 P0-5                                      |
| Missing D-Bus methods                        | **Closed** — 45/45 registered; 7 stubs **now implemented** (6561ef4)                                     | Updated §2.3 method matrix                             |
| ~325 vs ~40 properties                       | **Confirmed** — actual count is 321 `kWPANTUNDProperty_*` defines (the original ~325 estimate was close) | Updated P1-7 table; Rust has ~40 registered handlers   |
| PropGet string vs variant                    | **Real wire difference**                                                                                 | Documented as intentional deviation                    |
| No lifecycle (pid/chroot/privdrop)           | **Closed** — `lifecycle.rs` with `unlinkat(dirfd)` + `getpwnam_r`                                        | Updated §2.2 P1-4                                      |
| No GPIO write                                | **Closed** — `ncp_gpio::hard_reset()` wired into reset path                                              | Updated §2.2 P1-5                                      |
| No AutoAssociateAfterReset                   | **Closed** — sends `PROP_NET_STACK_UP=1` on `Initializing→Offline`                                       | Updated §2.2 P1-6                                      |
| No Pcap                                      | **Closed** — `pcap.rs` with `AtomicBool` + `spawn_blocking`                                              | Updated §2.2 P1-2                                      |

### 2026-07-15 (Milestone D closure — re-verified against committed code)

The implementation log previously marked Milestone C items as "uncommitted diff" and
Milestone D as "PARTIAL" with 7 D-Bus stubs + missing `NetworkTimeUpdate`. Git history
shows these were committed before this re-verification:

| Claim                                        | Result                                                                                                                   | Correction applied?                                                 |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| Milestone C "uncommitted diff"               | **Committed** — `c8f0a10` (resilience: lifecycle, NetworkRetain, Pcap, GPIO, AutoAssociate)                              | Updated §8 log (P0-3/P0-4/P0-5/P1-2/P1-4/P1-5/P1-6 → commit hashes) |
| P1-3 "7 D-Bus stubs"                         | **Closed** — `dcd085d` (+`6561ef4`) register and implement all 45; 7 former stubs route to real `commissioner_ops` tasks | Updated §2.3 matrix (no stubs remain)                               |
| P1-8 "NetworkTimeUpdate not present"         | **Closed** — `6561ef4` adds `signals::emit_network_time_update`, wired in `main.rs` + `base.rs`                          | Updated §2.2 P1-8                                                   |
| Milestone D "PARTIAL"                        | **Closed** — 45/45 methods registered AND implemented; signal present                                                    | Updated §4 Milestone D → DONE                                       |

**Honest answer to "is this ready for the implementor?"**

- **Yes** for prioritization, ownership of gaps, and starting **Milestones A+B+C+D**
  (P0-1 through P0-5, P1-2 through P1-9 — all closed).
- **No** for Milestone E (property inventory + StatCollector, P1-1) — needs live TI firmware
  inventory before expanding the handler map.
- **Deferred / non-blocking:** `dcu-serial` `system:`/`fd:`
  transports (not yet implemented), `IPv6PacketMatcher` not on the live path.
- **Do not** treat "100% of C" as "implement 321 properties and every
  Thread link-metrics path on day one" without product prioritization.