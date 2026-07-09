pub mod add_remove;
pub mod get;
pub mod help;
pub mod reset;
pub mod set;
pub mod status;

use crate::dbus_client::DbusClient;

#[derive(Debug)]
pub enum CommandError {
    NoCommand,
    UnknownCommand(String),
    MissingArgs {
        command: String,
        expected: &'static str,
    },
    Dbus(String),
    InvalidInput(String),
    Quit,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCommand => write!(f, "No command specified"),
            Self::UnknownCommand(name) => write!(f, "The command \"{name}\" is not recognised."),
            Self::MissingArgs { command, expected } => {
                write!(f, "{command}: missing required argument ({expected})")
            }
            Self::Dbus(msg) => write!(f, "{msg}"),
            Self::InvalidInput(msg) => write!(f, "{msg}"),
            Self::Quit => Ok(()),
        }
    }
}

impl std::error::Error for CommandError {}

pub async fn dispatch(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    let cmd = args.first().ok_or(CommandError::NoCommand)?;
    match *cmd {
        "get" => get::run(client, &args[1..]).await,
        "set" => set::run(client, &args[1..]).await,
        "add" => add_remove::run_insert(client, &args[1..]).await,
        "remove" => add_remove::run_remove(client, &args[1..]).await,
        "status" => status::run(client, &args[1..]).await,
        "reset" => reset::run(client, &args[1..]).await,
        "help" | "?" => help::run(client, &args[1..]).await,
        "quit" | "exit" | "q" => Err(CommandError::Quit),
        "clear" => {
            print!("\x1B[2J\x1B[H");
            Ok(String::new())
        }
        _ => Err(CommandError::UnknownCommand(cmd.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus_client::testutil::dummy_client;

    #[test]
    fn error_no_command_display() {
        let err = CommandError::NoCommand;
        assert_eq!(err.to_string(), "No command specified");
    }

    #[test]
    fn error_unknown_command_display() {
        let err = CommandError::UnknownCommand("foo".into());
        assert_eq!(err.to_string(), "The command \"foo\" is not recognised.");
    }

    #[test]
    fn error_missing_args_display() {
        let err = CommandError::MissingArgs {
            command: "set".into(),
            expected: "property-name",
        };
        assert_eq!(
            err.to_string(),
            "set: missing required argument (property-name)"
        );
    }

    #[test]
    fn error_dbus_display() {
        let err = CommandError::Dbus("connection refused".into());
        assert_eq!(err.to_string(), "connection refused");
    }

    #[test]
    fn error_quit_display() {
        let err = CommandError::Quit;
        assert_eq!(err.to_string(), "");
    }

    #[test]
    fn command_error_is_error() {
        let err: Box<dyn std::error::Error> = Box::new(CommandError::NoCommand);
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn dispatch_no_command() {
        let client = dummy_client().await;
        let result = dispatch(&client, &[]).await;
        assert!(matches!(result, Err(CommandError::NoCommand)));
    }

    #[tokio::test]
    async fn dispatch_unknown_command() {
        let client = dummy_client().await;
        let result = dispatch(&client, &["nonexistent"]).await;
        assert!(matches!(result, Err(CommandError::UnknownCommand(ref s)) if s == "nonexistent"));
    }

    #[tokio::test]
    async fn dispatch_quit() {
        let client = dummy_client().await;
        for cmd in &["quit", "exit", "q"] {
            let result = dispatch(&client, &[cmd]).await;
            assert!(
                matches!(result, Err(CommandError::Quit)),
                "command {cmd} should return Quit"
            );
        }
    }

    #[tokio::test]
    async fn dispatch_help_returns_output() {
        let client = dummy_client().await;
        let result = dispatch(&client, &["help"]).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Command List"));
    }

    #[tokio::test]
    async fn dispatch_help_question_mark() {
        let client = dummy_client().await;
        let result = dispatch(&client, &["?"]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Command List"));
    }

    #[tokio::test]
    async fn dispatch_clear_returns_empty() {
        let client = dummy_client().await;
        let result = dispatch(&client, &["clear"]).await;
        assert_eq!(result.unwrap(), "");
    }
}
