//! User-editable favorites.
//!
//! Owns the ordered `favorites.json` store that backs the volume switcher's "Favorites" section.
//! The store is the single source of truth; it replaces the previously hardcoded four favorites.
//! `volumes::get_favorites` (macOS) and `volumes_linux::get_favorites` (Linux) read this store and
//! map each entry to a `LocationInfo` with `category: Favorite`. Mutations go through the IPC
//! commands in `commands/favorites.rs`, which re-emit `volumes-changed` so both panes' switchers
//! update live.
//!
//! See `favorites/CLAUDE.md` for the seed-once contract and the FDA-pending skip.

pub mod store;
