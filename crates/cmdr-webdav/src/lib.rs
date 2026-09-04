#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to a WebDAV server.

#[cfg(test)]
use cmdr_webdav as _;

pub(crate) mod errors;
pub(crate) mod params;
pub(crate) mod propfind;
pub(crate) mod transport;
pub mod volume;

pub use errors::WebdavConnectError;
pub use params::WebdavConnectionParams;
pub use volume::{UnattendedReconnect, WebdavVolume, connect_webdav_volume};
