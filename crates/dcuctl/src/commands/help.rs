use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

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

pub async fn run(_client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    if let Some(cmd_name) = args.first() {
        match *cmd_name {
            "get" => Ok(super::get::HELP.to_string()),
            "set" => Ok(super::set::HELP.to_string()),
            "status" => Ok(super::status::HELP.to_string()),
            "reset" => Ok(super::reset::HELP.to_string()),
            "add" => Ok(
                "add - Used for adding values to macfilterlist\n\n\
                 Usage: add [-d] [-s] [-v value] <property-name> <property-value>\n\n\
                 Options:\n  -h, --help         Print help\n  -d, --data         Value is hex-encoded binary data\n  -s, --string       Value is a string (default)\n"
                    .to_string(),
            ),
            "remove" => Ok(
                "remove - Used for removing values to macfilterlist\n\n\
                 Usage: remove [-d] [-s] [-v value] <property-name> <property-value>\n\n\
                 Options:\n  -h, --help         Print help\n  -d, --data         Value is hex-encoded binary data\n  -s, --string       Value is a string (default)\n"
                    .to_string(),
            ),
            _ => Err(CommandError::UnknownCommand(cmd_name.to_string())),
        }
    } else {
        Ok(COMMAND_LIST.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus_client::testutil::dummy_client;

    #[tokio::test]
    async fn help_no_args_returns_command_list() {
        let client = dummy_client().await;
        let output = run(&client, &[]).await.unwrap();
        assert!(output.contains("get"));
        assert!(output.contains("set"));
        assert!(output.contains("status"));
        assert!(output.contains("reset"));
        assert!(output.contains("help"));
        assert!(output.contains("quit"));
    }

    #[tokio::test]
    async fn help_get_returns_get_help() {
        let client = dummy_client().await;
        let output = run(&client, &["get"]).await.unwrap();
        assert!(output.contains("get - Get a property"));
        assert!(output.contains("--all"));
        assert!(output.contains("--value-only"));
    }

    #[tokio::test]
    async fn help_set_returns_set_help() {
        let client = dummy_client().await;
        let output = run(&client, &["set"]).await.unwrap();
        assert!(output.contains("set - Set a property"));
        assert!(output.contains("--data"));
        assert!(output.contains("--string"));
    }

    #[tokio::test]
    async fn help_status_returns_status_help() {
        let client = dummy_client().await;
        let output = run(&client, &["status"]).await.unwrap();
        assert!(output.contains("status - Retrieve the status"));
    }

    #[tokio::test]
    async fn help_reset_returns_reset_help() {
        let client = dummy_client().await;
        let output = run(&client, &["reset"]).await.unwrap();
        assert!(output.contains("reset - Reset the border router"));
    }

    #[tokio::test]
    async fn help_unknown_command_returns_error() {
        let client = dummy_client().await;
        let result = run(&client, &["nonexistent"]).await;
        assert!(matches!(result, Err(CommandError::UnknownCommand(ref s)) if s == "nonexistent"));
    }

    #[tokio::test]
    async fn help_add_returns_add_help() {
        let client = dummy_client().await;
        let output = run(&client, &["add"]).await.unwrap();
        assert!(output.contains("add - Used for adding"));
    }

    #[tokio::test]
    async fn help_remove_returns_remove_help() {
        let client = dummy_client().await;
        let output = run(&client, &["remove"]).await.unwrap();
        assert!(output.contains("remove - Used for removing"));
    }
}
