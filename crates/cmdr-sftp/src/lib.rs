#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an SFTP server.

#[cfg(test)]
use cmdr_sftp as _;

pub mod auth;
pub(crate) mod errors;
pub(crate) mod extensions;
pub(crate) mod known_hosts;
pub(crate) mod params;
pub mod transport;
pub(crate) mod trust;
pub mod volume;

pub use errors::SftpConnectError;
pub use extensions::ServerExtensions;
pub use params::SftpConnectionParams;
pub use volume::{HostKeyApproval, SftpConnectOutcome, SftpVolume, connect_sftp_volume};
