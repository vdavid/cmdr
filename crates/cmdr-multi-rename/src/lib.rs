// `unused_crate_dependencies` is judged per compilation unit, so it lives here rather
// than in the workspace's lint table.
#![warn(unused_crate_dependencies)]

//! The Multi-Rename Tool's engine (⌃M, Total Commander's mask renamer), Tauri-free:
//! the mask, search & replace and the case steps, the preview's statuses, presets and
//! field history as pure list ops, the Results file, and [`MultiRename`]: the open
//! sheets' sessions, their media reads, and apply. A host ([`MultiRenameHost`]) answers
//! what differs between the desktop app (`apps/desktop/src-tauri/src/multi_rename/`)
//! and the browser demo, and keeps the files. See `DETAILS.md`.

pub mod error;
pub mod history;
mod host;
pub mod mask;
pub mod media;
pub mod names_file;
pub mod plan;
pub mod presets;
pub mod run;
pub mod session;
pub mod transform;
mod transliterate;

pub use host::{MultiRenameHost, RenamePlace};
pub use session::MultiRename;

#[cfg(test)]
mod history_test;
#[cfg(test)]
mod mask_test;
#[cfg(test)]
mod names_file_test;
#[cfg(test)]
mod plan_test;
#[cfg(test)]
mod presets_test;
#[cfg(test)]
mod transform_test;
#[cfg(test)]
mod transliterate_test;
