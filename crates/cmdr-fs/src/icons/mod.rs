//! Path → icon-key classification, the cheap half.
//!
//! Only the checks that cost nothing live here: a `HashMap` lookup against the
//! standard locations ([`special_folders`]) and a name-suffix test
//! ([`packages`]). Both run for every entry of a listing, so neither may touch
//! the disk. `FileEntry::new` calls them through `entry::get_icon_id`.
//!
//! Everything expensive — the NSWorkspace fetch, the `getxattr` custom-icon
//! probe, the disk cache, the frontend wiring — stays in the app's `icons`
//! module, which re-exports these two so its call sites read unchanged.

pub mod packages;
pub mod special_folders;
