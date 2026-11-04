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
//! #
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
//!     log::info!("{event}");
//! }
//! # }
//! ```
//!
//! ## Runtime
//!
//! [`tokio`](https://github.com/tokio-rs/tokio): the event channel is a bounded
//! `tokio::sync::mpsc` channel. Upstream also offered `async-std`; this fork dropped it.
//!
//! ## Changes from upstream
//!
//! This is Cmdr's fork of [`fsevent-stream`](https://github.com/PhotonQuantum/fsevent-stream),
//! maintained in the Cmdr monorepo since upstream went quiet. Behavior that differs from upstream:
//!
//! - **Overflow flag**: `EventStream` carries a shared flag that turns `true` whenever events are
//!   dropped (the channel is full, an event couldn't be built, or the callback panicked), so the
//!   consumer can rescan.
//! - **Unknown flag bits are kept**: `Event::flags` is parsed with `from_bits_retain`, so an event
//!   carrying a bit newer than this crate keeps its known bits. Upstream's `from_bits` dropped the
//!   whole event, silently.
//! - **Creation fails loudly**: an empty path list, a NULL stream, or a stream FSEvents won't start
//!   is an `Err`, where upstream went on with a NULL stream or left `abort` waiting forever.
//! - **No leaks**: the stream context is released on every error path, and the run-loop observer's
//!   context no longer leaks on each `abort`.
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

// The lint set this crate is held to lives in the workspace root's `[workspace.lints]`, opted
// into by `Cargo.toml`'s `lints.workspace = true`. `unused_crate_dependencies` can't go with them:
// it's judged per compilation unit, so it belongs on the lib alone.
#![warn(unused_crate_dependencies)]
pub mod stream;
#[macro_use]
pub mod ffi;
pub mod flags;
mod observer;
#[cfg(test)]
mod tests;
mod utils;
