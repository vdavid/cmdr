//! [![GitHub Workflow Status](https://img.shields.io/github/workflow/status/PhotonQuantum/fsevent-stream/Test?style=flat-square)](https://github.com/PhotonQuantum/fsevent-stream/actions/workflows/test.yml)
//! [![crates.io](https://img.shields.io/crates/v/fsevent-stream?style=flat-square)](https://crates.io/crates/fsevent-stream)
//! [![Documentation](https://img.shields.io/docsrs/fsevent-stream?style=flat-square)](https://docs.rs/fsevent-stream)
//!
//! Stream-based [`FSEvents`](https://developer.apple.com/documentation/coreservices/file_system_events) API bindings.
//!
//! ## Features
//!
//! - Support directory-granular and file-granular events.
//! - Retrieve related file inode with `kFSEventStreamCreateFlagUseExtendedData`.
//!
//! ## Example
//!
//! ```rust
//! use std::path::Path;
//! use std::time::Duration;
//!
//! use cmdr_fsevent_stream::ffi::{
//!     kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer,
//!     kFSEventStreamCreateFlagUseCFTypes, kFSEventStreamCreateFlagUseExtendedData,
//!     kFSEventStreamEventIdSinceNow,
//! };
//! use cmdr_fsevent_stream::stream::create_event_stream;
//! use futures_util::StreamExt;
//! # #[cfg(feature = "tokio")]
//! # use tokio1 as tokio;
//! # #[cfg(feature = "async-std")]
//! # use async_std1 as async_std;
//! #
//! # #[cfg(feature = "async-std")]
//! # #[async_std::main]
//! # async fn main() {
//! #     run().await;
//! # }
//! #
//! # #[cfg(feature = "tokio")]
//! # #[tokio::main]
//! # async fn main() {
//! #     run().await;
//! # }
//!
//! # async fn run() {
//! let (stream, handler) = create_event_stream(
//!     [Path::new(".")],
//!     kFSEventStreamEventIdSinceNow,
//!     Duration::ZERO,
//!     kFSEventStreamCreateFlagNoDefer
//!         | kFSEventStreamCreateFlagFileEvents
//!         | kFSEventStreamCreateFlagUseExtendedData
//!         | kFSEventStreamCreateFlagUseCFTypes,
//! )
//!     .expect("stream to be created");
//! # {
//! # let mut handler = handler;
//! # std::thread::spawn(move || {
//! #     handler.abort();
//! # });
//! # }
//!
//! let mut stream = stream.into_flatten();
//! while let Some(event) = stream.next().await {
//!     println!(
//!         "[{}] path: {:?}({}), flags: {} ({:x})",
//!         event.id,
//!         event.path,
//!         event.inode.unwrap_or(-1),
//!         event.flags,
//!         event.raw_flags
//!     );
//! }
//! # }
//! ```
//!
//! ## Runtime Support
//!
//! Both [`tokio`](https://github.com/tokio-rs/tokio) and [`async-std`](https://github.com/async-rs/async-std) are supported
//! via feature flags.
//!
//! `tokio` support is enabled by default. To enable `async-std` support, disable default features and enable `async-std`
//! feature.
//!
//! ## Acknowledgement
//!
//! Some code in this project is adapted from the following projects:
//!
//! - [fsevent-sys](https://github.com/octplane/fsevent-rust)
//! - [notify](https://github.com/notify-rs/notify)
//!
//! ## License
//!
//! This project is licensed under MIT License.

pub mod stream;
#[macro_use]
pub mod ffi;
pub mod flags;
mod observer;
#[cfg(test)]
mod tests;
mod utils;
