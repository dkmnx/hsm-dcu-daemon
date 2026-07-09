use zbus::zvariant::Value;

use crate::commands::CommandError;
use crate::dbus_client::DbusClient;
use crate::property_parser::parse_property_value;

pub(crate) const HELP: &str = "\
set - Set a property

Usage: set [-d] [-s] [-v value] <property-name> <property-value>

Options:
  -h, --help         Print help
  -d, --data         Value is hex-encoded binary data
  -s, --string       Value is a string (default)
  -v, --value VAL    Value to set (alternative to positional arg)
";

pub async fn run(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    run_updateprop(client, args, "set", "PropSet").await
}

pub(crate) async fn run_updateprop(
    client: &DbusClient,
    args: &[&str],
    command_name: &str,
    dbus_method: &str,
) -> Result<String, CommandError> {
    let mut is_data = false;
    let mut value_arg: Option<&str> = None;
    let mut positional = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-h" | "--help" => return Ok(HELP.to_string()),
            "-d" | "--data" => is_data = true,
            "-s" | "--string" => is_data = false,
            "-v" | "--value" => {
                i += 1;
                if i < args.len() {
                    value_arg = Some(args[i]);
                }
            }
            _ => positional.push(args[i]),
        }
        i += 1;
    }

    let property_name = positional.first().ok_or(CommandError::MissingArgs {
        command: command_name.to_string(),
        expected: "property-name",
    })?;

    let property_value = match value_arg {
        Some(v) => v,
        None => match positional.get(1) {
            Some(v) => v,
            None => "",
        },
    };

    if property_value.is_empty() && value_arg.is_none() {
        return Err(CommandError::MissingArgs {
            command: command_name.to_string(),
            expected: "property-value",
        });
    }

    let parsed_value: Value<'static> = if is_data {
        let hex: String = property_value
            .chars()
            .filter(|c| *c != ':' && *c != '-' && *c != ' ')
            .collect();
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for chunk in hex.as_bytes().chunks(2) {
            let s = std::str::from_utf8(chunk)
                .map_err(|e| CommandError::InvalidInput(e.to_string()))?;
            let byte =
                u8::from_str_radix(s, 16).map_err(|e| CommandError::InvalidInput(e.to_string()))?;
            bytes.push(byte);
        }
        Value::Array(zbus::zvariant::Array::from(
            bytes.into_iter().map(Value::U8).collect::<Vec<_>>(),
        ))
    } else {
        parse_property_value(property_name, property_value)
            .map_err(|e| CommandError::InvalidInput(e.to_string()))?
    };

    let owned_value = zbus::zvariant::OwnedValue::try_from(parsed_value)
        .map_err(|e| CommandError::Dbus(e.to_string()))?;

    let ret = match dbus_method {
        "PropSet" => client.prop_set(property_name, owned_value).await?,
        "PropInsert" => client.prop_insert(property_name, owned_value).await?,
        "PropRemove" => client.prop_remove(property_name, owned_value).await?,
        _ => return Err(CommandError::Dbus(format!("unknown method: {dbus_method}"))),
    };

    if ret != 0 {
        return Err(CommandError::Dbus(format!(
            "{dbus_method} failed with error {ret}"
        )));
    }

    Ok(String::new())
}
