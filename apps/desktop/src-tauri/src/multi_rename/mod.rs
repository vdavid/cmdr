//! The app's host for the Multi-Rename Tool (⌃M), whose pure core is
//! `crates/cmdr-multi-rename`: the sessions, the media reads, apply, and the app's files.
//! Everything the core exports is re-exported here, so the app names it as
//! `crate::multi_rename::…`. See `DETAILS.md`.

pub use cmdr_multi_rename::*;

pub(crate) mod app_files;
pub(crate) mod media;
pub(crate) mod preset_store;
pub(crate) mod run;
pub(crate) mod session;

#[cfg(test)]
mod app_files_test;
#[cfg(test)]
mod media_real_test;
#[cfg(test)]
mod media_test;
#[cfg(test)]
mod preset_store_test;
#[cfg(test)]
mod results_test;
#[cfg(test)]
mod run_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod wire_test;
