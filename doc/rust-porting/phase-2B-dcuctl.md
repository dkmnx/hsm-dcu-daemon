# Phase 2B: `dcuctl` — CLI Tool

## Overview

Port the interactive CLI tool. Talks to the daemon over D-Bus, not directly
to hardware. Only the 8 commands actually registered in the C dispatch table
are ported — the other ~25 command files are compiled but unreachable from
the REPL.

**Replaces**: `src/dcuctl/*.c` (registered commands only)

**Effort**: 3-5 days

## Scope

Only commands registered in `wpanctl.c` `commandList[]` (lines 128-135)
and `wpanctl-cmds.h` `WPANCTL_CLI_COMMANDS` macro (lines 38-68):

| Command   | Registered | Hidden | D-Bus Method        | Source File                          |
| --------- | ---------- | ------ | ------------------- | ------------------------------------ |
| `get`     | Yes        | No     | `PropGet`           | `tool-cmd-getprop.c` (325 LOC)       |
| `set`     | Yes        | No     | `PropSet`           | `tool-cmd-setprop.c` (31 LOC)        |
| `status`  | Yes        | No     | `Status`            | `tool-cmd-status.c` (153 LOC)        |
| `reset`   | Yes        | No     | `ResetNCP`          | `tool-cmd-reset.c` (158 LOC)         |
| `help`    | Yes        | No     | (none)              | `wpanctl.c` lines 98-118             |
| `quit`    | Yes        | No     | (none)              | `wpanctl.c` lines 173-177            |
| `clear`   | Yes        | No     | (none)              | `wpanctl.c` lines 120-126            |
| `add`     | Yes        | Yes    | `PropInsert`        | `tool-cmd-insertprop.c` (31 LOC)     |
| `remove`  | Yes        | Yes    | `PropRemove`        | `tool-cmd-removeprop.c` (31 LOC)     |
| `?`       | Yes        | Yes    | (none, alias help)  | `wpanctl.c` line 115                 |

**Total C code for registered commands**: ~1,100 LOC

The shared `tool_updateprop()` function (249 LOC) backs `set`, `add`, and
`remove` — only the D-Bus method name differs.

## Source Files to Port

| C File                             | LOC  | What to Extract                                        |
| ---------------------------------- | ---- | ------------------------------------------------------ |
| `src/dcuctl/wpanctl.c`             | 824  | Main loop, readline, dispatch, `help`, `clear`, `quit` |
| `src/dcuctl/wpanctl-utils.c`       | 759  | D-Bus helpers, property formatting                     |
| `src/dcuctl/tool-cmd-getprop.c`    | 325  | `get` command                                          |
| `src/dcuctl/tool-cmd-setprop.c`    | 31   | `set` command (delegates to updateprop)                |
| `src/dcuctl/tool-cmd-status.c`     | 153  | `status` command                                       |
| `src/dcuctl/tool-cmd-reset.c`      | 158  | `reset` command                                        |
| `src/dcuctl/tool-cmd-insertprop.c` | 31   | `add` command (hidden, delegates)                      |
| `src/dcuctl/tool-cmd-removeprop.c` | 31   | `remove` command (hidden, delegates)                   |
| `src/dcuctl/tool-updateprop.c`     | 249  | Shared set/insert/remove implementation                |

**Total C source**: ~2,560 LOC

## Crate Structure

```text
dcuctl/
├── Cargo.toml
└── src/
    ├── main.rs              # Entry point, CLI args, TTY detection
    ├── dbus_client.rs       # Thin D-Bus client wrapper
    ├── repl.rs              # Readline REPL loop + non-TTY fallback
    ├── property_parser.rs   # Parse property values from user input
    ├── property_formatter.rs # Format property values for display
    └── commands/
        ├── mod.rs           # Enum dispatch + shared helpers
        ├── get.rs           # get / getprop
        ├── set.rs           # set / setprop (delegates to shared)
        ├── status.rs        # status
        ├── reset.rs         # reset
        ├── help.rs          # help, ?
        └── add_remove.rs    # add / remove (hidden, shared impl)
```

## D-Bus Client Architecture

dcuctl is a D-Bus **client**. It connects to the daemon's well-known name
and calls methods on the per-interface object path. The `dcu-dbus` crate
is a **server** — dcuctl does NOT depend on it.

```rust
// dbus_client.rs

use zbus::Proxy;
use zbus::zvariant::{Value, OwnedObjectPath};

const WPANTUND_DBUS_NAME: &str = "com.nestlabs.WPANTunnelDriver";
const WPANTUND_DBUS_INTERFACE: &str = "com.nestlabs.WPANTunnelDriver";
const WPANTUND_DBUS_PATH: &str = "/com/nestlabs/WPANTunnelDriver";

pub struct DbusClient {
    conn: zbus::Connection,
    interface_name: String,
    /// Per-interface D-Bus name resolved via GetInterfaces.
    iface_bus_name: String,
    /// Object path: /com/nestlabs/WPANTunnelDriver/<interface_name>
    iface_path: String,
}

impl DbusClient {
    /// Connect to the system bus and resolve the interface.
    pub async fn connect(interface: &str) -> Result<Self, DbusError> {
        let conn = zbus::Connection::system().await?;
        // 1. Call GetInterfaces on the base path to find our interface
        // 2. Extract the per-interface bus name
        // 3. Build the object path
        // 4. Verify D-Bus version (GetVersion)
        todo!()
    }

    /// Build a proxy for the current interface.
    fn proxy(&self) -> Result<Proxy<'_>, DbusError> {
        ProxyBuilder::new(&self.conn)
            .destination(&self.iface_bus_name)?
            .path(&self.iface_path)?
            .interface(WPANTUND_DBUS_INTERFACE)?
            .cache_properties(CacheProperties::No)
            .build()
            .map_err(Into::into)
    }

    // --- Typed methods matching C D-Bus calls ---

    pub async fn prop_get(&self, name: &str) -> Result<String, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("PropGet", &(name,)).await?;
        msg.body().deserialize().map_err(Into::into)
    }

    pub async fn prop_set(&self, name: &str, value: Value<'_>) -> Result<i32, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("PropSet", &(name, value)).await?;
        msg.body().deserialize().map_err(Into::into)
    }

    pub async fn prop_insert(&self, name: &str, value: Value<'_>) -> Result<i32, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("PropInsert", &(name, value)).await?;
        msg.body().deserialize().map_err(Into::into)
    }

    pub async fn prop_remove(&self, name: &str, value: Value<'_>) -> Result<i32, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("PropRemove", &(name, value)).await?;
        msg.body().deserialize().map_err(Into::into)
    }

    pub async fn status(&self) -> Result<String, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("Status", &()).await?;
        msg.body().deserialize().map_err(Into::into)
    }

    pub async fn reset_ncp(&self) -> Result<i32, DbusError> {
        let p = self.proxy()?;
        let msg = p.call_method("ResetNCP", &()).await?;
        let ret: i32 = msg.body().deserialize()?;
        // C suppresses error code 6 for reset
        Ok(if ret == 6 { 0 } else { ret })
    }

    /// Get the list of available interfaces.
    pub async fn get_interfaces(&self) -> Result<Vec<(String, String)>, DbusError> {
        // Call GetInterfaces on the base object path (not per-interface)
        todo!()
    }
}
```

### Error Handling

The C code uses integer error codes with `print_error_diagnosis()` (50+
cases). The Rust version maps D-Bus errors to `DbusError` and prints
diagnostic messages for common failures:

- `kWPANTUNDStatus_InterfaceNotFound` — "Interface not managed by daemon"
- `kWPANTUNDStatus_Busy` / `-EBUSY` — "NCP busy"
- `kWPANTUNDStatus_NCP_Crashed` — "NCP has crashed"
- `kWPANTUNDStatus_InvalidWhenDisabled` — "Interface is disabled"
- `kWPANTUNDStatus_JoinFailedAtScan` — "Join failed at scan stage"
- `kWPANTUNDStatus_JoinFailedAtAuthenticate` — "Join failed at authentication"

Error code 6 is **silently suppressed** (treated as success) in `reset`,
matching the C behavior at `tool-cmd-reset.c:140`.

## Detailed File Specs

### `main.rs`

```rust
use clap::Parser;
use std::io::{self, BufRead, IsTerminal};

#[derive(Parser)]
#[command(name = "dcuctl")]
#[command(about = "Wi-SUN FAN Border Router Control Tool")]
struct Cli {
    /// Interface name (default: wfan0)
    #[arg(short = 'I', default_value = "wfan0")]
    interface: String,

    /// Suppress version check against daemon
    #[arg(short = 'i')]
    ignore_mismatch: bool,

    /// Print debug output
    #[arg(short = 'd')]
    debug: bool,

    /// Run a single command and exit (trailing args)
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Connect to D-Bus
    let client = match dbus_client::DbusClient::connect(&cli.interface).await {
        Ok(c) => c,
        Err(e) => { eprintln!("Error: {e}"); std::process::exit(1); }
    };

    // Version check (unless --ignore-mismatch)
    if !cli.ignore_mismatch {
        // wpan_dbus_version_check() — compare client vs daemon version
    }

    if cli.command.is_empty() {
        // TTY detection: readline vs fgets
        if io::stdin().is_terminal() {
            repl::run_interactive(&client).await;
        } else {
            repl::run_batch(&client).await;
        }
    } else {
        let args: Vec<&str> = cli.command.iter().map(|s| s.as_str()).collect();
        commands::dispatch(&client, &args).await;
    }
}
```

### `repl.rs`

```rust
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

const MAX_LINE: usize = 200;

/// Interactive mode with readline and history.
pub async fn run_interactive(client: &DbusClient) {
    let mut rl = DefaultEditor::new().unwrap();
    // History file from $WPANCTL_HISTORY_FILE, default ~/.wpanctl_history
    let history_path = std::env::var("WPANCTL_HISTORY_FILE")
        .unwrap_or_else(|_| format!("{}/.wpanctl_history", std::env::var("HOME").unwrap()));
    let _ = rl.load_history(&history_path);

    loop {
        let prompt = format!("dcuctl:{} > ", client.interface_name());
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() { continue; }
                if line == "exit" || line == "quit" || line == "q" { break; }

                let _ = rl.add_history_entry(line);
                let parts: Vec<&str> = line.split_whitespace().collect();
                match commands::dispatch(client, &parts).await {
                    Ok(output) if !output.is_empty() => println!("{output}"),
                    Err(e) => eprintln!("Error: {e}"),
                    _ => {}
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => { eprintln!("Error: {e}"); break; }
        }
    }
    let _ = rl.save_history(&history_path);
}

/// Non-TTY mode: fgets with 200-byte buffer (matches C line 707).
pub async fn run_batch(client: &DbusClient) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.len() >= MAX_LINE {
            eprintln!("Error: line too long (max {MAX_LINE})");
            continue;
        }
        let line = line.trim();
        if line.is_empty() { continue; }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match commands::dispatch(client, &parts).await {
            Ok(output) if !output.is_empty() => println!("{output}"),
            Err(e) => eprintln!("Error: {e}"),
            _ => {}
        }
    }
}
```

### `commands/mod.rs`

Enum-based dispatch (not trait objects — simpler, zero-cost, exhaustive match):

```rust
use crate::dbus_client::DbusClient;

#[derive(Debug)]
pub enum CommandError {
    NoCommand,
    UnknownCommand(String),
    MissingArgs { command: &'static str, expected: &'static str },
    Dbus(crate::dbus_client::DbusError),
    InvalidInput(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCommand => write!(f, "No command specified"),
            Self::UnknownCommand(name) => write!(f, "The command \"{name}\" is not recognised."),
            Self::MissingArgs { command, expected } =>
                write!(f, "{command}: missing required argument ({expected})"),
            Self::Dbus(e) => write!(f, "{e}"),
            Self::InvalidInput(msg) => write!(f, "{msg}"),
        }
    }
}

/// Dispatch a parsed command line to the appropriate handler.
pub async fn dispatch(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    let cmd = args.first().ok_or(CommandError::NoCommand)?;
    match *cmd {
        "get"    => get::run(client, &args[1..]).await,
        "set"    => set::run(client, &args[1..]).await,
        "add"    => add_remove::run_insert(client, &args[1..]).await,
        "remove" => add_remove::run_remove(client, &args[1..]).await,
        "status" => status::run(client, &args[1..]).await,
        "reset"  => reset::run(client, &args[1..]).await,
        "help"   => help::run(client, &args[1..]).await,
        "?"      => help::run(client, &args[1..]).await,
        "clear"  => { print!("\x1B[2J\x1B[H"); Ok(String::new()) }
        _ => Err(CommandError::UnknownCommand(cmd.into())),
    }
}

/// Execute a single command line (used by batch mode and -c flag).
pub async fn run_single(client: &DbusClient, args: &[str]) {
    let parts: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match dispatch(client, &parts).await {
        Ok(output) if !output.is_empty() => println!("{output}"),
        Err(e) => eprintln!("Error: {e}"),
        _ => {}
    }
}
```

### `get.rs` — `get` command

Source: `tool-cmd-getprop.c` (325 LOC)

```rust
/// Usage: get [-a] [-v] [-t timeout_ms] [<property-name>]
///
/// Options:
///   -a, --all          Get all properties
///   -v, --value-only   Print value without property name prefix
///   -t, --timeout MS   D-Bus call timeout in milliseconds
///
/// Without a property name, behaves like -a (get all).
/// Special handling for "connecteddevices" property.
pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Parse options: -a, -v, -t <ms>
    // If no property name after options: get all properties
    // For each property, call client.prop_get(name)
    // Format: "<property_name> = <value>\n"
    // If -v: just the value, no property name prefix
    todo!()
}
```

**D-Bus method**: `PropGet` with property name string argument.
**Return format**: `"<property_name> = <value>\n"` per property.

**Special case**: `"connecteddevices"` property uses temp file parsing
and IP extraction (C code `tool-cmd-getprop.c:276-302`). This should be
handled in the daemon, not the CLI — the Rust CLI just prints the value.

### `set.rs` — `set` command

Source: `tool-cmd-setprop.c` (31 LOC) + `tool-updateprop.c` (249 LOC)

```rust
/// Usage: set [-d] [-s] [-v value] [-t timeout_ms] <property-name> <property-value>
///
/// Options:
///   -d, --data         Value is hex-encoded binary data
///   -s, --string       Value is a string (default)
///   -v, --value VAL    Value to set (alternative to positional arg)
///   -t, --timeout MS   D-Bus call timeout in milliseconds (default: 30000)
///
/// The property value type is inferred from the property name, or
/// overridden by -d/-s flags.
pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Parse options: -d, -s, -v, -t
    // Remaining args: <property-name> [<property-value>]
    // Parse property value using property_parser::parse_property_value()
    // Call client.prop_set(name, value)
    // Return empty string on success
    todo!()
}
```

**Shared with `add`/`remove`**: The `tool_updateprop()` C function is
shared by set/insert/remove. In Rust, the shared logic lives in a helper
module; `set.rs`, `add_remove.rs` call it with different D-Bus methods.

### `status.rs` — `status` command

Source: `tool-cmd-status.c` (153 LOC)

```rust
/// Usage: status [-t timeout_ms]
///
/// Display daemon and NCP status for the current interface.
///
/// Output format (must match C exactly):
///   wfan0 => [
///       "NCP:State" => "offline"
///       "Daemon:Enabled" => true
///       "NCP:Version" => "TIWISUNFAN/1.0.2; RELEASE; Dec 19 2024 21:44:28"
///       ...
///   ]
pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Parse options: -t
    // Call client.status()
    // The D-Bus Status method returns a dict of all properties
    // Format as: "<interface> => [\n    "<key>" => <value>\n    ...\n]"
    todo!()
}
```

**Output format**: `"<interface_name> => [\n    \"<key>\" => <value>\n    ...\n]"`

The `dump_info_from_iter()` function in `wpanctl-utils.c:45-158` defines
the formatting rules per D-Bus type:

| D-Bus Type     | Format                  | Example              |
| -------------- | ----------------------- | -------------------- |
| `STRING`       | `"%s"` (quoted)         | `"hello"`            |
| `BYTE`         | `0x%02X`                | `0xFF`               |
| `UINT16`       | `0x%04X`                | `0x001A`             |
| `INT16`        | `%d` (decimal)          | `-5`                 |
| `UINT32`       | `%d` (decimal, not hex) | `100`                |
| `INT32`        | `%d`                    | `-1`                 |
| `UINT64`       | `0x%016llX`             | `0x00000000000000FF` |
| `BOOLEAN`      | `"true"` / `"false"`    |                      |
| `ARRAY(bytes)` | `[XX XX XX...]`         | `[01 02 03]`         |
| `ARRAY(other)` | `[\n  elem\n  ...\n]`   | multiline            |
| `DICT_ENTRY`   | `key => value`          |                      |
| `VARIANT`      | unwrapped inner value   |                      |

### `reset.rs` — `reset` command

Source: `tool-cmd-reset.c` (158 LOC)

```rust
/// Usage: reset [-t timeout_ms]
///
/// Reset the NCP. Prints "Resetting NCP. . ." to stderr.
///
/// Note: Error code 6 is silently suppressed (treated as success),
/// matching C behavior at tool-cmd-reset.c:140.
pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Parse options: -t
    // eprintln!("Resetting NCP. . .");
    // Call client.reset_ncp() — already handles error code 6 suppression
    // Return empty string on success
    todo!()
}
```

### `help.rs` — `help` and `?` commands

Source: `wpanctl.c` lines 98-118

```rust
/// Usage: help [<command>]
///
/// Without arguments: list all commands with descriptions.
/// With a command name: execute that command with --help.
pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    if let Some(cmd_name) = args.first() {
        // Execute the named command with ["--help"] and return its output
        match *cmd_name {
            "get"    => get::run(client, &["--help"]).await,
            "set"    => set::run(client, &["--help"]).await,
            "status" => status::run(client, &["--help"]).await,
            "reset"  => reset::run(client, &["--help"]).await,
            "add"    => add_remove::help_insert(),
            "remove" => add_remove::help_remove(),
            _ => Err(CommandError::UnknownCommand(cmd_name.into())),
        }
    } else {
        // Print command list (matching C print_commands())
        Ok(COMMAND_LIST.to_string())
    }
}

const COMMAND_LIST: &str = "\
Command List:
  get          Get a property
  set          Set a property
  add          Used for adding values to macfilterlist (hidden)
  remove       Used for removing values to macfilterlist (hidden)
  status       Retrieve the status of the interface.
  reset        Reset the border router
  help         Display this help
  quit         Terminate command line mode.
";
```

### `add_remove.rs` — `add` and `remove` (hidden)

Source: `tool-cmd-insertprop.c` (31 LOC) + `tool-cmd-removeprop.c` (31 LOC)

These delegate to the shared `tool_updateprop()` logic, differing only
in the D-Bus method name (`PropInsert` vs `PropRemove`).

```rust
/// Usage: add [-d] [-s] [-v value] [-t timeout_ms] <property-name> <property-value>
pub async fn run_insert(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Same as set, but calls client.prop_insert()
    todo!()
}

/// Usage: remove [-d] [-s] [-v value] [-t timeout_ms] <property-name> <property-value>
pub async fn run_remove(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    // Same as set, but calls client.prop_remove()
    todo!()
}

pub fn help_insert() -> Result<String, CommandError> {
    Ok("Used for adding values to macfilterlist\n\nUsage: add [-d] [-s] [-v value] <property-name> <property-value>".into())
}

pub fn help_remove() -> Result<String, CommandError> {
    Ok("Used for removing values to macfilterlist\n\nUsage: remove [-d] [-s] [-v value] <property-name> <property-value>".into())
}
```

## Property Formatting Rules

### `property_parser.rs`

Parse user input into `zbus::zvariant::Value` based on property name:

```rust
use zbus::zvariant::Value;

pub fn parse_property_value(name: &str, input: &str) -> Result<Value<'static>, ParseError> {
    match name {
        "Network:PANID" => parse_hex_u16(input),
        "NCP:CCAThreshold" => parse_i8(input),
        "UnicastChList" | "BroadcastChList" | "AsyncChList" => parse_channel_mask(input),
        "Interface:Up" | "Stack:Up" => parse_bool(input),
        "NCP:HardwareAddress" => parse_eui64(input),
        "IPv6:LinkLocalAddress" | "IPv6:MeshLocalAddress" => parse_ipv6(input),
        "Network:XPANID" => parse_hex_u64(input),
        "Network:NodeType" => parse_node_type(input),
        "NCP:Channel" => parse_u8(input),
        "NCP:Frequency" => parse_u32(input),
        "NCP:RSSI" => parse_i32(input),
        "NCP:TXPower" => parse_f64(input),
        _ => Ok(Value::Str(input.into())),
    }
}
```

### Node Type Parsing

From `wpanctl-utils.c:358-401`, `parse_node_type()` accepts these aliases:

| Input                                          | Result                |
| ---------------------------------------------- | --------------------- |
| `router`, `r`, `2`                             | `"router"`            |
| `end-device`, `end`, `ed`, `e`, `3`            | `"end-device"`        |
| `sleepy-end-device`, `sleepy`, `sed`, `s`, `4` | `"sleepy-end-device"` |
| `lurker`, `nl-lurker`, `l`, `6`                | `"nl-lurker"`         |

Default (no type specified): `"router"` for `form`, `"end-device"` for `join`.

### `property_formatter.rs`

Format `Value` for display, matching C output exactly:

```rust
use zbus::zvariant::Value;

pub fn format_property(name: &str, value: &Value) -> String {
    match name {
        "NCP:HardwareAddress" => format_eui64(value),
        "NCP:ExtendedAddress" | "NCP:MACAddress" => format_eui64(value),
        "UnicastChList" | "BroadcastChList" | "AsyncChList" => format_channel_mask(value),
        "Network:PANID" => format!("0x{:04X}", value_as_u16(value)),
        "Network:XPANID" => format!("0x{:016X}", value_as_u64(value)),
        "NCP:Channel" => value.to_string(),
        "NCP:Frequency" => value.to_string(),
        "NCP:CCAThreshold" => value.to_string(),
        "NCP:TXPower" => value.to_string(),
        "Interface:Up" | "Stack:Up" => format_boolean(value),
        "Daemon:Enabled" => format_boolean(value),
        _ => value.to_string(),
    }
}

/// Format EUI-64 as [XXXXXXXXXXXXXXXX] (no colons, uppercase hex).
fn format_eui64(value: &Value) -> String {
    // Match C: "[00124B0014F7D2E6]"
    todo!()
}

/// Format channel mask as colon-separated hex bytes: "ff:ff:01"
fn format_channel_mask(value: &Value) -> String {
    // Match C: "ff:ff:01"
    todo!()
}

/// Format boolean as "true" or "false" (lowercase, no quotes).
fn format_boolean(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}
```

### Connected Devices Special Case

The C `get` command has elaborate special handling for `"connecteddevices"`
(`tool-cmd-getprop.c:276-302`): it writes to a temp file, parses IP
addresses, polls until "Last IPs" line. This logic belongs in the daemon,
not the CLI. The Rust CLI should just print the value returned by D-Bus.

## Tests

### Test 1: Command Dispatch

```rust
#[tokio::test]
async fn unknown_command_returns_error() {
    let client = mock_client().await;
    let result = dispatch(&client, &["nonexistent"]).await;
    assert!(matches!(result, Err(CommandError::UnknownCommand(_))));
}

#[tokio::test]
async fn empty_args_returns_no_command() {
    let client = mock_client().await;
    let result = dispatch(&client, &[]).await;
    assert!(matches!(result, Err(CommandError::NoCommand)));
}
```

### Test 2: Argument Parsing

```rust
#[test]
fn set_property_parsing() {
    let args = vec!["Network:PANID", "0xABCD"];
    // Verify parse_property_value produces correct Value
    let v = parse_property_value("Network:PANID", "0xABCD").unwrap();
    assert_eq!(v, Value::U16(0xABCD));
}

#[test]
fn parse_bool_variants() {
    assert_eq!(parse_property_value("Interface:Up", "true").unwrap(), Value::Bool(true));
    assert_eq!(parse_property_value("Interface:Up", "1").unwrap(), Value::Bool(true));
    assert_eq!(parse_property_value("Interface:Up", "false").unwrap(), Value::Bool(false));
    assert_eq!(parse_property_value("Interface:Up", "0").unwrap(), Value::Bool(false));
}
```

### Test 3: Property Value Formatting

```rust
#[test]
fn format_panid() {
    let value = Value::U16(0xABCD);
    assert_eq!(format_property("Network:PANID", &value), "0xABCD");
}

#[test]
fn format_eui64() {
    let value = Value::Str("[00124B0014F7D2E6]".into());
    assert_eq!(format_property("NCP:HardwareAddress", &value), "[00124B0014F7D2E6]");
}

#[test]
fn format_channel_mask() {
    let value = Value::Array(zbus::zvariant::Array::from(vec![
        Value::U8(0xFF), Value::U8(0xFF), Value::U8(0x01),
    ]));
    assert_eq!(format_property("UnicastChList", &value), "ff:ff:01");
}

#[test]
fn format_boolean() {
    assert_eq!(format_property("Interface:Up", &Value::Bool(true)), "true");
    assert_eq!(format_property("Interface:Up", &Value::Bool(false)), "false");
}
```

### Test 4: Node Type Parsing

```rust
#[test]
fn parse_node_type_router_aliases() {
    for input in &["router", "r", "2"] {
        assert_eq!(parse_node_type(input).unwrap(), "router");
    }
}

#[test]
fn parse_node_type_end_device_aliases() {
    for input in &["end-device", "enddevice", "end", "ed", "e", "3"] {
        assert_eq!(parse_node_type(input).unwrap(), "end-device");
    }
}

#[test]
fn parse_node_type_sleepy_aliases() {
    for input in &["sleepy-end-device", "sleepy", "sed", "s", "4"] {
        assert_eq!(parse_node_type(input).unwrap(), "sleepy-end-device");
    }
}

#[test]
fn parse_node_type_lurker_aliases() {
    for input in &["lurker", "nl-lurker", "l", "6"] {
        assert_eq!(parse_node_type(input).unwrap(), "nl-lurker");
    }
}
```

### Test 5: Help Text

```rust
#[test]
fn all_commands_have_help() {
    let commands = vec!["get", "set", "status", "reset", "help", "add", "remove"];
    for name in commands {
        let result = help::run(&mock_client(), &[name]).await;
        assert!(result.is_ok(), "Command {name} missing help text");
        assert!(!result.unwrap().is_empty());
    }
}
```

### Test 6: REPL Completion

```rust
#[test]
fn command_completion_prefix() {
    let completions = complete_command("st");
    assert_eq!(completions, vec!["status", "set"]);
}

#[test]
fn command_completion_exact() {
    let completions = complete_command("status");
    assert_eq!(completions, vec!["status"]);
}

#[test]
fn command_completion_hidden_commands_excluded() {
    // Hidden commands (add, remove) should NOT appear in completion
    let completions = complete_command("a");
    assert!(completions.is_empty() || !completions.contains(&"add".to_string()));
}
```

### Test 7: Status Output Format

```rust
#[test]
fn status_output_matches_c_format() {
    // Verify the exact output format matches the C version
    let output = format_status_output("wfan0", &mock_status_dict());
    let expected = "wfan0 => [\n\
        \x20   \"NCP:State\" => \"offline\"\n\
        \x20   \"Daemon:Enabled\" => true\n\
        \x20   \"NCP:Version\" => \"TIWISUNFAN/1.0.2\"\n\
        ]";
    assert_eq!(output, expected);
}
```

### Test 8: D-Bus Value Formatting

```rust
#[test]
fn format_uint32_as_decimal() {
    // C uses %d for UINT32, not hex
    let value = Value::U32(100);
    assert_eq!(variant_to_string(&value), "100");
}

#[test]
fn format_uint64_as_hex() {
    let value = Value::U64(0xFF);
    assert_eq!(variant_to_string(&value), "0x00000000000000FF");
}

#[test]
fn format_byte_array_inline() {
    // C formats byte arrays as [XX XX XX] inline
    let value = Value::Array(zbus::zvariant::Array::from(vec![
        Value::U8(0x01), Value::U8(0x02), Value::U8(0x03),
    ]));
    assert_eq!(format_byte_array(&value), "[01 02 03]");
}
```

## Dependencies

```toml
[dependencies]
wisun-types = { path = "../wisun-types" }
clap = { version = "4", features = ["derive"] }
rustyline = "14"
zbus = "4"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "io-util", "time"] }
tracing = "0.1"
thiserror = "2"
```

Note: `dcu-dbus` is NOT a dependency. dcuctl is a pure D-Bus client;
`dcu-dbus` is the server they talk to.

## Verification Checklist

- [ ] All 8 registered commands implemented: `get`, `set`, `status`, `reset`, `help`, `quit`/`exit`/`q`, `clear`, plus hidden `add`/`remove`
- [ ] `?` alias for `help` works
- [ ] `status` output matches C version character-for-character
- [ ] `get` command handles all properties from `dcu-dbus` properties module
- [ ] `set`/`add`/`remove` validate property types
- [ ] Error code 6 suppressed in `reset` (matches C behavior)
- [ ] Tab completion works for command names (excluding hidden commands)
- [ ] Prompt format: `"dcuctl:<interface> > "` (with space before `>`)
- [ ] History file: `$WPANCTL_HISTORY_FILE` or `~/.wpanctl_history`
- [ ] Non-TTY mode: `fgets` with 200-byte line buffer
- [ ] TTY detection: readline when terminal, fgets otherwise
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] No `unsafe` code in this crate
