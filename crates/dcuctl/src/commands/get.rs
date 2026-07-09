use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

pub(crate) const HELP: &str = "\
get - Get a property

Usage: get [-a] [-v] [<property-name>]

Options:
  -a, --all          Get all properties
  -v, --value-only   Print only the value without the property name prefix

Without a property name, all properties are retrieved.
";

pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    let mut get_all = false;
    let mut value_only = false;
    let mut positional = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-h" | "--help" => return Ok(HELP.to_string()),
            "-a" | "--all" => get_all = true,
            "-v" | "--value-only" => value_only = true,
            _ => positional.push(args[i]),
        }
        i += 1;
    }

    if get_all || positional.is_empty() {
        let props = [
            "NCP:State",
            "NCP:Version",
            "NCP:ProtocolVersion",
            "NCP:InterfaceType",
            "NCP:HardwareAddress",
            "NCP:ExtendedAddress",
            "NCP:MACAddress",
            "NCP:CCAThreshold",
            "NCP:TXPower",
            "NCP:Region",
            "NCP:ModeID",
            "NCP:Channel",
            "NCP:Frequency",
            "NCP:RSSI",
            "Network:Name",
            "Network:PANID",
            "Network:XPANID",
            "Network:NodeType",
            "Network:IsCommissioned",
            "Network:IsConnected",
            "Network:Key",
            "IPv6:LinkLocalAddress",
            "IPv6:MeshLocalAddress",
            "IPv6:MeshLocalPrefix",
            "Interface:Up",
            "Stack:Up",
            "Daemon:Version",
            "Daemon:Enabled",
            "Daemon:ReadyForHostSleep",
        ];
        let mut output = String::new();
        for key in &props {
            match client.prop_get(key).await {
                Ok(value) => {
                    if value_only {
                        output.push_str(&value);
                        output.push('\n');
                    } else {
                        output.push_str(key);
                        output.push_str(" = ");
                        output.push_str(&value);
                        output.push('\n');
                    }
                }
                Err(e) => {
                    eprintln!("{key}: {e}");
                }
            }
        }
        Ok(output)
    } else {
        let mut output = String::new();
        for key in &positional {
            let value = client.prop_get(key).await?;
            if value_only {
                output.push_str(&value);
                output.push('\n');
            } else {
                output.push_str(key);
                output.push_str(" = ");
                output.push_str(&value);
                output.push('\n');
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus_client::testutil::dummy_client;

    #[tokio::test]
    async fn get_help() {
        let client = dummy_client().await;
        let output = run(&client, &["--help"]).await.unwrap();
        assert!(output.contains("get - Get a property"));
        assert!(output.contains("--all"));
        assert!(output.contains("--value-only"));
    }

    #[tokio::test]
    async fn get_all_returns_ok_even_when_props_fail() {
        // With a dummy client, all prop_get calls fail, but get -a
        // should still return Ok (errors are printed to stderr, not propagated).
        let client = dummy_client().await;
        let result = run(&client, &["-a"]).await;
        assert!(result.is_ok());
        // Output will be empty since all D-Bus calls fail
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_single_prop_propagates_error() {
        // get <prop> (without -a) propagates errors via `?`
        let client = dummy_client().await;
        let result = run(&client, &["NCP:State"]).await;
        assert!(result.is_err());
    }
}
