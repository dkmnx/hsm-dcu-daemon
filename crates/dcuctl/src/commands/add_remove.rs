use crate::commands::CommandError;
use crate::dbus_client::DbusClient;

pub async fn run_insert(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    super::set::run_updateprop(client, args, "add", "PropInsert").await
}

pub async fn run_remove(client: &DbusClient, args: &[&str]) -> Result<String, CommandError> {
    super::set::run_updateprop(client, args, "remove", "PropRemove").await
}
