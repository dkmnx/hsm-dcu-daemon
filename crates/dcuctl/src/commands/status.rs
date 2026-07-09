use std::collections::BTreeMap;

use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

pub(crate) const HELP: &str = "\
status - Retrieve the status of the interface

Usage: status

Options:
  -h, --help         Print help
";

pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    for &arg in args {
        if arg == "-h" || arg == "--help" {
            return Ok(HELP.to_string());
        }
    }

    let props = client.status().await?;
    Ok(format_status_output(client.interface_name(), &props))
}

/// Format status properties into the C-compatible output:
///
/// ```text
/// wfan0 => [
///     "NCP:State" => "offline"
///     "Daemon:Enabled" => true
/// ]
/// ```
pub fn format_status_output(
    interface: &str,
    props: &std::collections::HashMap<String, String>,
) -> String {
    let sorted: BTreeMap<&str, &String> = props.iter().map(|(k, v)| (k.as_str(), v)).collect();

    let mut output = format!("{interface} => [\n");
    for (key, value) in &sorted {
        output.push_str(&format!("    \"{key}\" => {value}\n"));
    }
    output.push(']');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn format_empty_status() {
        let props = HashMap::new();
        let output = format_status_output("wfan0", &props);
        assert_eq!(output, "wfan0 => [\n]");
    }

    #[test]
    fn format_status_sorted_by_key() {
        let mut props = HashMap::new();
        props.insert("NCP:State".into(), "offline".into());
        props.insert("Daemon:Enabled".into(), "true".into());
        props.insert("NCP:Version".into(), "TIWISUNFAN/1.0.2".into());

        let output = format_status_output("wfan0", &props);
        let expected = "\
wfan0 => [
    \"Daemon:Enabled\" => true
    \"NCP:State\" => offline
    \"NCP:Version\" => TIWISUNFAN/1.0.2
]";
        assert_eq!(output, expected);
    }

    #[test]
    fn format_status_custom_interface() {
        let mut props = HashMap::new();
        props.insert("NCP:State".into(), "associated".into());

        let output = format_status_output("wpan0", &props);
        assert!(output.starts_with("wpan0 => ["));
    }
}
