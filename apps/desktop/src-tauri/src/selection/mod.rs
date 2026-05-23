//! Selection dialog backend (M5).
//!
//! Mirrors `crate::search` but narrower: there is no scope, no system-dir exclusion,
//! and no in-memory index. The selection matcher runs in JS against the focused folder's
//! entries; this module only owns:
//!
//! - `history.rs`: persistent recent-selections store (separate file from search history,
//!   same atomic-write story, narrower entry schema).
//! - `ai/`: AI prompt + parser + query builder for natural-language → glob/regex.
//!   The model receives a sample of the focused folder's filenames so it can infer a
//!   pattern from intent like "all rymdskottkärra files".
//!
//! See `apps/desktop/src-tauri/src/selection/CLAUDE.md` for the why behind the split
//! and the design tradeoffs.

pub mod ai;
pub mod history;
