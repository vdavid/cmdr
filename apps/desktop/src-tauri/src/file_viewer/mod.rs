//! The app's host for the file viewer, whose core is `crates/cmdr-viewer`: the app's
//! one `Viewer` and its answers to the host seams (`app_host.rs`), the `cmdr-media://`
//! scheme handler, the Tauri events, "Open with", the pull-to-temp materializer, and
//! analytics. Everything the core exports is re-exported here, so the app names it as
//! `crate::file_viewer::…`.

pub use cmdr_viewer::*;

pub(crate) mod analytics;
mod app_host;
mod events;
mod markdown_image_fetch;
pub(crate) mod materialize;
pub mod media_protocol;
mod open_with_copies;
pub(crate) mod open_with_extract;

#[cfg(test)]
mod analytics_test;
#[cfg(test)]
mod log_view_test;
#[cfg(test)]
mod markdown_image_fetch_test;
#[cfg(test)]
mod markdown_session_test;
#[cfg(test)]
mod materialize_test;
#[cfg(test)]
mod media_protocol_test;
#[cfg(test)]
mod media_volume_test;
#[cfg(test)]
mod open_with_extract_test;
#[cfg(test)]
mod smb_open_test;
#[cfg(test)]
mod snapshot_open_test;
#[cfg(test)]
mod wire_events_test;

pub use app_host::{app_viewer, init_app_handle};
pub use events::{ViewerMediaLoad, ViewerPullProgress};
pub use materialize::init_materialize_dir;
pub use open_with_extract::init_open_with_extract_dir;
