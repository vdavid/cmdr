//! What the UI calls an SMB share: `{share} on {server}`, the ONE spelling the volume
//! switcher, the breadcrumb, favorites, saved rows, and Ask Cmdr's facts share.
//!
//! ❗ The server is the name a person typed in the Add server sheet when they named it
//! ("private on Home NAS", #913), else the address it goes by (with the port off 445).
//! The address stays in Finder's Get Info and in the hub's server row; a name is a label
//! only, ❌ never an identity (`server_identity`).

use crate::network::known_shares::KnownNetworkShare;
use crate::network::server_identity::{SmbServer, smb_server};

/// A mounted share's label, read off its mount: `server` and `port` as `statfs` gave them.
pub(crate) fn mounted(share: &str, server: &str, port: u16) -> String {
    let typed = crate::network::manual_servers::typed_name(&[SmbServer::new(server, port)]);
    let name =
        typed.unwrap_or_else(|| smb_server(&crate::network::smb_server_address::friendly_server_name(server), port));
    format!("{share} on {name}")
}

/// A saved share row's label, for a share nobody has mounted this session too.
pub(crate) fn saved(row: &KnownNetworkShare) -> String {
    filed(&row.share_name, &row.server_name, row.address.as_deref(), row.port)
}

/// The label of `share` filed under `server_name` (a discovery name, `host` or
/// `host:port`), mounted from `address` on `port` when a mount said so.
pub(crate) fn filed(share: &str, server_name: &str, address: Option<&str>, port: Option<u16>) -> String {
    let typed = crate::network::manual_servers::typed_name(&row_servers(server_name, address, port));
    format!("{share} on {}", typed.as_deref().unwrap_or(server_name))
}

/// Every server a row can be recognized by: the name it was filed under (its port and
/// all), and where its mount dialed.
fn row_servers(server_name: &str, address: Option<&str>, port: Option<u16>) -> Vec<SmbServer> {
    let filed = SmbServer::from_name(server_name);
    let mut servers = vec![filed.clone()];
    if let Some(address) = address {
        servers.push(SmbServer::new(address, port.unwrap_or(filed.port())));
    }
    servers
}

/// [`filed`] over a typed-name table and `hosts`, so a test needs no store.
#[cfg(test)]
pub(crate) fn filed_among(
    named: &[(SmbServer, String)],
    share: &str,
    server_name: &str,
    address: Option<&str>,
    port: Option<u16>,
    hosts: &[crate::network::NetworkHost],
) -> String {
    let typed =
        crate::network::manual_servers::typed_name_among(named, &row_servers(server_name, address, port), hosts);
    format!("{share} on {}", typed.as_deref().unwrap_or(server_name))
}

#[cfg(test)]
#[path = "smb_share_label_test.rs"]
mod tests;
