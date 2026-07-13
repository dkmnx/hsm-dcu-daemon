mod commands;
mod dbus_client;
mod property_parser;

use std::io::{self, BufRead, IsTerminal};

use clap::Parser;

use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

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

    /// Run a single command and exit (trailing args)
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let client = match DbusClient::connect(&cli.interface).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    if !cli.ignore_mismatch {
        // The daemon's `GetVersion` returns the **protocol** version
        // (a u32, currently 2 — WPANTUND_DBUS_VERSION), not a semver
        // string, so it cannot equal the CLI crate version. We only warn
        // if the daemon reports an unexpected protocol number.
        if let Ok(version) = client.get_version().await {
            const EXPECTED_PROTOCOL: u32 = 2;
            if version != EXPECTED_PROTOCOL {
                eprintln!(
                    "WARNING: Protocol version mismatch: dcuctl expects {EXPECTED_PROTOCOL}, daemon reports {version}"
                );
            }
        }
    }

    if cli.command.is_empty() {
        if io::stdin().is_terminal() {
            run_interactive(&client).await;
        } else {
            run_batch(&client).await;
        }
    } else {
        let args: Vec<&str> = cli.command.iter().map(|s| s.as_str()).collect();
        execute(&client, &args).await;
    }
}

/// Returns `true` if the REPL should exit (quit/exit/q).
async fn execute(client: &DbusClient, args: &[&str]) -> bool {
    match commands::dispatch(client, args).await {
        Ok(output) if !output.is_empty() => {
            println!("{output}");
            false
        }
        Err(CommandError::Quit) => true,
        Err(e) => {
            eprintln!("Error: {e}");
            false
        }
        _ => false,
    }
}

async fn run_interactive(client: &DbusClient) {
    let mut rl = rustyline::DefaultEditor::new().expect("failed to create readline editor");

    let history_path = std::env::var("WPANCTL_HISTORY_FILE").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.wpanctl_history")
    });
    let _ = rl.load_history(&history_path);

    loop {
        let prompt = format!("dcuctl:{} > ", client.interface_name());
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line);
                let parts: Vec<&str> = line.split_whitespace().collect();
                if execute(client, &parts).await {
                    break;
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("Error: {e}");
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
}

async fn run_batch(client: &DbusClient) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() >= 200 {
            eprintln!("Error: line too long (max 200)");
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if execute(client, &parts).await {
            break;
        }
    }
}
