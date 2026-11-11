//! The app's host for the Multi-Rename Tool (⌃M), whose engine is
//! `crates/cmdr-multi-rename`: the app's engine and its host answers (`app_host`), and
//! the app's files (`app_files`, `preset_store`).
//! Everything the core exports is re-exported here, so the app names it as
//! `crate::multi_rename::…`. See `DETAILS.md`.

pub use cmdr_multi_rename::*;

pub(crate) mod analytics;
pub(crate) mod app_files;
pub(crate) mod app_host;
pub(crate) mod preset_store;

#[cfg(test)]
mod analytics_test;
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
