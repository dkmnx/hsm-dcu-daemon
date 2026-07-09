use std::collections::HashMap;

use zbus::Proxy;
use zbus::zvariant::OwnedValue;

use crate::commands::CommandError;

const WPANTUND_DBUS_NAME: &str = "com.nestlabs.WPANTunnelDriver";
const WPANTUND_DBUS_INTERFACE: &str = "com.nestlabs.WPANTunnelDriver";
const WPANTUND_DBUS_PATH: &str = "/com/nestlabs/WPANTunnelDriver";

fn to_dbus_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::Dbus(e.to_string())
}

pub struct DbusClient {
    pub(crate) conn: zbus::Connection,
    pub(crate) interface_name: String,
    pub(crate) iface_bus_name: String,
    pub(crate) iface_path: String,
}

/// Parse the `GetInterfaces` reply into `(interface_name, bus_name)` pairs.
async fn parse_interfaces(conn: &zbus::Connection) -> Result<Vec<(String, String)>, CommandError> {
    let proxy = build_base_proxy(conn).await?;

    let result: Vec<OwnedValue> = proxy
        .call_method("GetInterfaces", &())
        .await
        .map_err(to_dbus_err)?
        .body()
        .deserialize()
        .map_err(to_dbus_err)?;

    let mut interfaces = Vec::new();
    for item in result {
        let pair: Vec<OwnedValue> = item
            .try_into()
            .map_err(|e: zbus::zvariant::Error| to_dbus_err(e))?;
        if pair.len() >= 2 {
            let mut iter = pair.into_iter();
            let name: Result<String, _> = iter.next().unwrap().try_into();
            let bus_name: Result<String, _> = iter.next().unwrap().try_into();
            if let (Ok(name), Ok(bus_name)) = (name, bus_name) {
                interfaces.push((name, bus_name));
            }
        }
    }
    Ok(interfaces)
}

impl DbusClient {
    pub async fn connect(interface: &str) -> Result<Self, CommandError> {
        let conn = zbus::Connection::system().await.map_err(to_dbus_err)?;

        let interfaces = parse_interfaces(&conn).await?;
        let iface_bus_name = interfaces
            .iter()
            .find(|(name, _)| name == interface)
            .map(|(_, bus)| bus.clone())
            .ok_or_else(|| {
                CommandError::Dbus(format!(
                    "Interface \"{interface}\" not found. Use `list` to see available interfaces."
                ))
            })?;
        let iface_path = format!("{WPANTUND_DBUS_PATH}/{interface}");

        Ok(Self {
            conn,
            interface_name: interface.to_string(),
            iface_bus_name,
            iface_path,
        })
    }

    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    async fn iface_proxy(&self) -> Result<Proxy<'_>, CommandError> {
        let proxy: Proxy<'_> = zbus::ProxyBuilder::new(&self.conn)
            .destination(&*self.iface_bus_name)
            .map_err(to_dbus_err)?
            .path(&*self.iface_path)
            .map_err(to_dbus_err)?
            .interface(WPANTUND_DBUS_INTERFACE)
            .map_err(to_dbus_err)?
            .cache_properties(zbus::CacheProperties::No)
            .build()
            .await
            .map_err(to_dbus_err)?;
        Ok(proxy)
    }

    pub async fn prop_get(&self, name: &str) -> Result<String, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p
            .call_method("PropGet", &(name,))
            .await
            .map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }

    pub async fn prop_set(&self, name: &str, value: OwnedValue) -> Result<i32, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p
            .call_method("PropSet", &(name, value))
            .await
            .map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }

    pub async fn prop_insert(&self, name: &str, value: OwnedValue) -> Result<i32, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p
            .call_method("PropInsert", &(name, value))
            .await
            .map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }

    pub async fn prop_remove(&self, name: &str, value: OwnedValue) -> Result<i32, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p
            .call_method("PropRemove", &(name, value))
            .await
            .map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }

    pub async fn status(&self) -> Result<HashMap<String, String>, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p.call_method("Status", &()).await.map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }

    pub async fn reset_ncp(&self) -> Result<i32, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p.call_method("ResetNCP", &()).await.map_err(to_dbus_err)?;
        let ret: i32 = msg.body().deserialize().map_err(to_dbus_err)?;
        // C suppresses error code 6 (tool-cmd-reset.c:140)
        Ok(if ret == 6 { 0 } else { ret })
    }

    pub async fn get_version(&self) -> Result<String, CommandError> {
        let p = self.iface_proxy().await?;
        let msg = p
            .call_method("GetVersion", &())
            .await
            .map_err(to_dbus_err)?;
        msg.body().deserialize().map_err(to_dbus_err)
    }
}

async fn build_base_proxy(conn: &zbus::Connection) -> Result<Proxy<'_>, CommandError> {
    let proxy: Proxy<'_> = zbus::ProxyBuilder::new(conn)
        .destination(WPANTUND_DBUS_NAME)
        .map_err(to_dbus_err)?
        .path(WPANTUND_DBUS_PATH)
        .map_err(to_dbus_err)?
        .interface(WPANTUND_DBUS_INTERFACE)
        .map_err(to_dbus_err)?
        .cache_properties(zbus::CacheProperties::No)
        .build()
        .await
        .map_err(to_dbus_err)?;
    Ok(proxy)
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// Create a dummy client for tests that don't make D-Bus calls.
    /// Must be called from within a tokio runtime.
    pub(crate) async fn dummy_client() -> DbusClient {
        let conn = zbus::Connection::session()
            .await
            .expect("failed to create test D-Bus connection");
        DbusClient {
            conn,
            interface_name: "wfan0".into(),
            iface_bus_name: "com.nestlabs.WPANTunnelDriver.test".into(),
            iface_path: "/com/nestlabs/WPANTunnelDriver/wfan0".into(),
        }
    }
}
