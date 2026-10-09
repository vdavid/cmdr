//! User-editable favorites.
//!
//! Owns the ordered `favorites.json` store that backs the frontend's favorites menu (⌃D), and what
//! a favorite row tells the frontend about where it points. Discovery (`volumes::get_favorites`,
//! `volumes_linux::get_favorites`) maps each stored entry to a `LocationInfo` with
//! `category: Favorite` (`target.rs`), and the listing's reach pass (`reach.rs`) decides whether a
//! pick can get there. Mutations go through the IPC commands in `commands/favorites.rs`, which
//! re-emit `volumes-changed` so both panes' menus update live.
//!
//! See `favorites/CLAUDE.md` for the seed-once contract and the read side's rules.

pub(crate) mod reach;
pub mod store;
pub mod target;
