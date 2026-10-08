#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an S3-compatible object store.

#[cfg(test)]
use cmdr_s3 as _;

pub mod cost;
pub(crate) mod encoding;
pub(crate) mod error;
pub(crate) mod metadata;
pub(crate) mod multipart;
pub(crate) mod ops;
pub(crate) mod params;
pub(crate) mod profile;
pub(crate) mod refusal;
pub(crate) mod request;
pub(crate) mod routing;
pub(crate) mod sigv4;
pub(crate) mod transport;
pub mod volume;
pub(crate) mod xml;

#[cfg(feature = "fuzzing")]
pub mod fuzzing;

pub use params::{InvalidProvider, S3ConnectionParams, S3Provider};
pub use refusal::S3ConnectError;
pub use volume::{S3Volume, UnattendedReconnect, connect_s3_volume};
