#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an SFTP server.

#[cfg(test)]
use cmdr_sftp as _;

pub mod auth;
pub mod errors;
pub mod known_hosts;
pub mod transport;
pub mod trust;
pub mod volume;

pub use errors::SftpConnectError;
pub use volume::{SftpConnectOutcome, SftpConnectionParams, SftpVolume, connect_sftp_volume};
