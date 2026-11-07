//! Drops from other apps that carry no file URLs: a browser image, a Mail attachment,
//! a Photos item. They arrive as file promises (or bare image bytes), which wry's drop
//! event can't see (it reads file URLs only, so its `paths` come back empty). This
//! module tells the frontend what a drag carries, receives the promised files into a
//! per-drop folder on drop, and removes that folder when the copy out of it ends.
//!
//! See `CLAUDE.md` beside this file.

pub mod events;
pub mod folders;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
mod payload;
#[cfg(target_os = "macos")]
mod tally;

pub use folders::init_drop_folder_root;
