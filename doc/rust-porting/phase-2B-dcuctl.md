# Phase 2B: `dcuctl` — CLI Tool

## Overview

Port the interactive CLI tool. Talks to the daemon over D-Bus, not directly to hardware. ~30 commands, each a small module.

**Replaces**: `src/dcuctl/*.c` (~30 command files)

**Effort**: 5-7 days

## Source Files to Port

| C File                                  | LOC  | What to Extract                    |
| --------------------------------------- | ---- | ---------------------------------- |
| `src/dcuctl/wpanctl.c`                  | 824  | Main loop, readline, dispatch      |
| `src/dcuctl/wpanctl-utils.c`            | ~400 | D-Bus helpers, property formatting |
| `src/dcuctl/tool-cmd-getprop.c`         | ~200 | `get` command                      |
| `src/dcuctl/tool-cmd-setprop.c`         | ~150 | `set` command                      |
| `src/dcuctl/tool-cmd-status.c`          | ~150 | `status` command                   |
| `src/dcuctl/tool-cmd-scan.c`            | ~120 | `scan` command                     |
| `src/dcuctl/tool-cmd-join.c`            | ~100 | `join` command                     |
| `src/dcuctl/tool-cmd-form.c`            | ~100 | `form` command                     |
| `src/dcuctl/tool-cmd-leave.c`           | ~60  | `leave` command                    |
| `src/dcuctl/tool-cmd-reset.c`           | ~50  | `reset` command                    |
| `src/dcuctl/tool-cmd-list.c`            | ~80  | `list` command                     |
| `src/dcuctl/tool-cmd-cd.c`              | ~60  | `cd` command                       |
| `src/dcuctl/tool-cmd-permit-join.c`     | ~80  | `permit-join` command              |
| `src/dcuctl/tool-cmd-config-gateway.c`  | ~120 | `config-gateway` command           |
| `src/dcuctl/tool-cmd-add-prefix.c`      | ~80  | `add-prefix` command               |
| `src/dcuctl/tool-cmd-remove-prefix.c`   | ~80  | `remove-prefix` command            |
| `src/dcuctl/tool-cmd-add-route.c`       | ~80  | `add-route` command                |
| `src/dcuctl/tool-cmd-remove-route.c`    | ~80  | `remove-route` command             |
| `src/dcuctl/tool-cmd-dataset.c`         | ~150 | `dataset` command                  |
| `src/dcuctl/tool-cmd-commissioner.c`    | ~120 | `commissioner` command             |
| `src/dcuctl/tool-cmd-joiner.c`          | ~120 | `joiner` command                   |
| `src/dcuctl/tool-cmd-mfg.c`             | ~100 | `mfg` command                      |
| `src/dcuctl/tool-cmd-peek.c`            | ~80  | `peek` command                     |
| `src/dcuctl/tool-cmd-poke.c`            | ~80  | `poke` command                     |
| `src/dcuctl/tool-cmd-pcap.c`            | ~100 | `pcap` command                     |
| `src/dcuctl/tool-cmd-poll.c`            | ~60  | `poll` command                     |
| `src/dcuctl/tool-cmd-resume.c`          | ~60  | `resume` command                   |
| `src/dcuctl/tool-cmd-begin-low-power.c` | ~60  | `begin-low-power` command          |
| `src/dcuctl/tool-cmd-begin-net-wake.c`  | ~60  | `begin-net-wake` command           |
| `src/dcuctl/tool-cmd-host-did-wake.c`   | ~60  | `host-did-wake` command            |
| `src/dcuctl/tool-cmd-linkmetrics.c`     | ~100 | `link-metrics` command             |
| `src/dcuctl/tool-cmd-mlr.c`             | ~100 | `mlr` command                      |
| `src/dcuctl/tool-cmd-bbr.c`             | ~100 | `bbr` command                      |
| `src/dcuctl/tool-cmd-add-service.c`     | ~80  | `add-service` command              |
| `src/dcuctl/tool-cmd-remove-service.c`  | ~80  | `remove-service` command           |
| `src/dcuctl/tool-cmd-insertprop.c`      | ~60  | `insertprop` command               |
| `src/dcuctl/tool-cmd-removeprop.c`      | ~60  | `removeprop` command               |
| `src/dcuctl/tool-updateprop.c`          | ~80  | `updateprop` command               |

**Total C code**: ~3,500 LOC (for the actual logic, ~12,500 with headers/duplication)

## Crate Structure

```text
dcuctl/
├── Cargo.toml
└── src/
    ├── main.rs             # Entry point, CLI args
    ├── repl.rs             # Readline REPL loop
    ├── property_parser.rs  # Parse property values
    ├── property_formatter.rs  # Format property values for display
    └── commands/
        ├── mod.rs          # Command trait + dispatch
        ├── status.rs
        ├── getprop.rs
        ├── setprop.rs
        ├── scan.rs
        ├── join.rs
        ├── form.rs
        ├── leave.rs
        ├── reset.rs
        ├── list.rs
        ├── cd.rs
        ├── permit_join.rs
        ├── config_gateway.rs
        ├── add_prefix.rs
        ├── remove_prefix.rs
        ├── add_route.rs
        ├── remove_route.rs
        ├── dataset.rs
        ├── commissioner.rs
        ├── joiner.rs
        ├── mfg.rs
        ├── peek.rs
        ├── poke.rs
        ├── pcap.rs
        ├── poll.rs
        ├── resume.rs
        ├── begin_low_power.rs
        ├── begin_net_wake.rs
        ├── host_did_wake.rs
        ├── link_metrics.rs
        ├── mlr.rs
        ├── bbr.rs
        ├── add_service.rs
        └── remove_service.rs
```

## Detailed File Specs

### `main.rs`

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "dcuctl")]
#[command(about = "Wi-SUN FAN Border Router Control Tool")]
struct Cli {
    /// Interface name (default: wfan0)
    #[arg(short = 'I', default_value = "wfan0")]
    interface: String,

    /// D-Bus address (default: system bus)
    #[arg(long)]
    dbus_address: Option<String>,

    /// Run a single command and exit
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    if cli.command.is_empty() {
        repl::run(&cli.interface);
    } else {
        commands::run_single(&cli.interface, &cli.command);
    }
}
```

### `repl.rs`

```rust
use rustyline::DefaultEditor;
use rustyline::completion::Completer;

pub fn run(interface: &str) {
    let mut rl = DefaultEditor::new().unwrap();
    let prompt = format!("dcuctl:{interface}> ");

    loop {
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() { continue; }
                if line == "exit" || line == "quit" { break; }

                let parts: Vec<&str> = line.split_whitespace().collect();
                match commands::dispatch(interface, &parts) {
                    Ok(output) => println!("{output}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => { eprintln!("Error: {e}"); break; }
        }
    }
}
```

### `commands/mod.rs`

```rust
pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn run(&self, interface: &str, args: &[&str]) -> Result<String, CommandError>;
    fn help(&self) -> &str;
}

pub fn dispatch(interface: &str, args: &[&str]) -> Result<String, CommandError> {
    let cmd_name = args.first().ok_or(CommandError::NoCommand)?;
    let cmd = get_command(cmd_name)?;
    cmd.run(interface, &args[1..])
}

fn get_command(name: &str) -> Result<Box<dyn Command>, CommandError> {
    match name {
        "status" => Ok(Box::new(status::StatusCommand)),
        "get" => Ok(Box::new(getprop::GetPropCommand)),
        "set" => Ok(Box::new(setprop::SetPropCommand)),
        "scan" => Ok(Box::new(scan::ScanCommand)),
        "join" => Ok(Box::new(join::JoinCommand)),
        "form" => Ok(Box::new(form::FormCommand)),
        "leave" => Ok(Box::new(leave::LeaveCommand)),
        "reset" => Ok(Box::new(reset::ResetCommand)),
        "list" => Ok(Box::new(list::ListCommand)),
        "cd" => Ok(Box::new(cd::CdCommand)),
        // ... all ~30 commands
        _ => Err(CommandError::UnknownCommand(name.into())),
    }
}
```

### Example: `status.rs`

```rust
pub struct StatusCommand;

impl Command for StatusCommand {
    fn name(&self) -> &str { "status" }
    fn description(&self) -> &str { "Display daemon and NCP status" }

    fn run(&self, interface: &str, _args: &[&str]) -> Result<String, CommandError> {
        // Call D-Bus Status method
        // Format output to match C version exactly:
        // wfan0 => [
        //     "NCP:State" => "offline"
        //     "Daemon:Enabled" => true
        //     "NCP:Version" => "TIWISUNFAN/1.0.2; RELEASE; Dec 19 2024 21:44:28"
        //     ...
        // ]
    }

    fn help(&self) -> &str {
        "Display daemon and NCP status\n\nUsage: status"
    }
}
```

### `property_parser.rs`

Parse property values from user input:

```rust
pub fn parse_property_value(name: &str, input: &str) -> Result<Variant, ParseError> {
    match name {
        "Network:PANID" => parse_hex_u16(input),
        "NCP:CCAThreshold" => parse_i32(input),
        "unicastchlist" | "broadcastchlist" => parse_channel_mask(input),
        "Interface:Up" | "Stack:Up" => parse_bool(input),
        "NCP:HardwareAddress" => parse_eui64(input),
        "IPv6:LinkLocalAddress" => parse_ipv6(input),
        // ... all property types
        _ => Ok(Variant::Str(input.into())),
    }
}
```

### `property_formatter.rs`

Format property values for display:

```rust
pub fn format_property(name: &str, value: &Variant) -> String {
    match name {
        "NCP:HardwareAddress" => format_eui64(value),
        "unicastchlist" => format_channel_mask(value),
        "Network:PANID" => format!("0x{:04X}", value.as_u16().unwrap()),
        "ch0centerfreq" => format!("{{{} MHz, {} kHz}}", mhz, khz),
        // ... match C output format exactly
        _ => value.to_string(),
    }
}
```

## Tests

### Test 1: Command Dispatch

```rust
#[test]
fn status_command_dispatches() {
    let cmd = get_command("status").unwrap();
    assert_eq!(cmd.name(), "status");
}
```

### Test 2: Argument Parsing

```rust
#[test]
fn set_property_parsing() {
    let cmd = SetPropCommand;
    let args = vec!["Network:PANID", "0xABCD"];
    assert!(cmd.validate_args(&args).is_ok());
}
```

### Test 3: Property Value Formatting

```rust
#[test]
fn format_panid() {
    let value = Variant::U16(0xABCD);
    let formatted = format_property("Network:PANID", &value);
    assert_eq!(formatted, "0xABCD");
}

#[test]
fn format_eui64() {
    let value = Variant::Bytes(vec![0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
    let formatted = format_property("NCP:HardwareAddress", &value);
    assert_eq!(formatted, "[00124B0014F7D2E6]");
}

#[test]
fn format_channel_mask() {
    let value = Variant::Bytes(vec![0xFF, 0xFF, 0x01]);
    let formatted = format_property("unicastchlist", &value);
    assert_eq!(formatted, "ff:ff:01");
}
```

### Test 4: Property Value Parsing

```rust
#[test]
fn parse_panid_hex() {
    let v = parse_property_value("Network:PANID", "0xABCD").unwrap();
    assert_eq!(v.as_u16(), Some(0xABCD));
}

#[test]
fn parse_bool_variants() {
    assert_eq!(parse_property_value("Interface:Up", "true").unwrap(), Variant::Bool(true));
    assert_eq!(parse_property_value("Interface:Up", "1").unwrap(), Variant::Bool(true));
    assert_eq!(parse_property_value("Interface:Up", "false").unwrap(), Variant::Bool(false));
}
```

### Test 5: Status Output Format

```rust
#[test]
fn status_output_matches_c() {
    let output = format_status_output(&mock_status());
    let expected = r#"wfan0 => [
    "NCP:State" => "offline"
    "Daemon:Enabled" => true
    "NCP:Version" => "TIWISUNFAN/1.0.2; RELEASE; Dec 19 2024 21:44:28"
]"#;
    assert_eq!(output, expected);
}
```

### Test 6: Help Text

```rust
#[test]
fn all_commands_have_help() {
    let commands = vec![
        "status", "get", "set", "scan", "join", "form", "leave",
        "reset", "list", "cd", "permit-join", "config-gateway",
        "add-prefix", "remove-prefix", "add-route", "remove-route",
        "dataset", "commissioner", "joiner", "mfg", "peek", "poke",
        "pcap", "poll", "resume", "begin-low-power", "begin-net-wake",
        "host-did-wake", "link-metrics", "mlr", "bbr",
        "add-service", "remove-service",
    ];
    for name in commands {
        let cmd = get_command(name).unwrap();
        assert!(!cmd.help().is_empty(), "Command {name} missing help text");
    }
}
```

### Test 7: REPL Completion

```rust
#[test]
fn command_completion() {
    let completions = complete_command("sta");
    assert_eq!(completions, vec!["status"]);
}
```

## Dependencies

```toml
[dependencies]
dcu-dbus = { path = "../dcu-dbus" }
wisun-types = { path = "../wisun-types" }
clap = { version = "4", features = ["derive"] }
rustyline = "14"
zbus = "4"
tracing = "0.1"
thiserror = "2"
```

## Verification Checklist

- [ ] All ~30 commands from C version are implemented
- [ ] `status` output matches C version character-for-character
- [ ] `get` command handles all 40+ properties
- [ ] `set` command validates property types
- [ ] Tab completion works for command names and property names
- [ ] Help text exists for every command
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] No `unsafe` code in this crate
