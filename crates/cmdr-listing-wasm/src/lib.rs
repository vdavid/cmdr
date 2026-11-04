//! Cmdr's directory listing engine in a browser: `cmdr-listing` over an
//! `InMemoryVolume`, answering the listing commands the way the desktop's Tauri
//! commands do. The website's `/demo` routes those commands here (#302).
//!
//! The engine's second host, beside `apps/desktop/src-tauri/src/listing_host/`:
//! [`tree`] builds the volume, [`events`] turns engine facts into Tauri-shaped
//! events, [`commands`] answers the commands, and `bindings` (browser only) is
//! the `wasm-bindgen` surface. Everything but `bindings` builds natively too, so
//! the tests run on the host. `CLAUDE.md` has the must-knows, `DETAILS.md` the JS
//! API and the build.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

#[cfg(target_family = "wasm")]
mod bindings;
pub mod commands;
pub mod events;
#[cfg(target_family = "wasm")]
mod rt_self_test;
pub mod tree;

#[cfg(test)]
mod commands_test;
#[cfg(test)]
mod events_test;
#[cfg(test)]
mod tree_test;
