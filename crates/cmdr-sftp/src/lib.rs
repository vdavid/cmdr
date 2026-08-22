#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an SFTP server.

#[cfg(test)]
use cmdr_sftp as _;

pub mod auth;
pub mod errors;
pub mod extensions;
pub mod known_hosts;
pub mod params;
pub mod transport;
pub mod trust;
pub mod volume;

pub use errors::SftpConnectError;
pub use extensions::ServerExtensions;
pub use params::SftpConnectionParams;
pub use volume::{SftpConnectOutcome, SftpVolume, connect_sftp_volume};
