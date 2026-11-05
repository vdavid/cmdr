//! The desktop's arena loader: a volume's index database read into `cmdr-search`'s
//! arena (`load.rs`), and the catch-up that keeps a warm one honest. The arena itself,
//! its row, and its builder are the crate's (`crates/cmdr-search/src/arena.rs`).

mod load;

#[cfg(test)]
mod load_db_tests;

pub use cmdr_search::arena::{ARENA_ROOT_ID, ArenaBuilder, OptU64, SearchEntry, SearchIndex};
pub(crate) use load::{catch_up_search_index, load_search_index};
