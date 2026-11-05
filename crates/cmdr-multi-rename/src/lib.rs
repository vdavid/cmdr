// `unused_crate_dependencies` is judged per compilation unit, so it lives here rather
// than in the workspace's lint table.
#![warn(unused_crate_dependencies)]

//! The Multi-Rename Tool's engine (⌃M, Total Commander's mask renamer), Tauri-free:
//! the mask, search & replace and the case steps, the preview's statuses, presets and
//! field history as pure list ops, and the Results file. A host keeps the files and the
//! sessions: the desktop app in `apps/desktop/src-tauri/src/multi_rename/`. See
//! `DETAILS.md`.

pub mod error;
pub mod history;
pub mod mask;
pub mod names_file;
pub mod plan;
pub mod presets;
pub mod transform;
mod transliterate;

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
