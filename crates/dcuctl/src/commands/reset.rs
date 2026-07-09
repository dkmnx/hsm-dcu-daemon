use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

pub(crate) const HELP: &str = "\
reset - Reset the border router

Usage: reset

Options:
  -h, --help         Print help
";

pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    for &arg in args {
        if arg == "-h" || arg == "--help" {
            return Ok(HELP.to_string());
        }
    }

    eprintln!("Resetting NCP. . .");

    let ret = client.reset_ncp().await?;

    if ret != 0 {
        return Err(CommandError::Dbus(format!(
            "ResetNCP failed with error {ret}"
        )));
    }

    Ok(String::new())
}
