//! Fixtures a host's tests and benches build arenas from: the hand-written fixture arena
//! and a synthetic home-folder-shaped one. Gated on `any(test, feature = "testing")`.

use std::collections::HashMap;

use crate::arena::{ARENA_ROOT_ID as ROOT_ID, ArenaBuilder, SearchIndex};
use crate::ranking::ImportanceWeights;
use crate::seam::{DirSizeSource, SourceError};

pub use crate::arena::{FixtureRow, arena_of};

/// Folder sizes held in a map by entry id: the [`DirSizeSource`] a test pairs with a
/// [`crate::MemorySource`]. A folder missing from the map has no size on record.
#[derive(Debug, Default, Clone)]
pub struct FolderSizes(pub HashMap<i64, u64>);

impl DirSizeSource for FolderSizes {
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError> {
        Ok(self
            .0
            .iter()
            .filter(|&(_, &size)| min.is_none_or(|min| size >= min) && max.is_none_or(|max| size <= max))
            .map(|(&id, &size)| (id, size))
            .collect())
    }

    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError> {
        Ok(ids.iter().map(|id| self.0.get(id).copied()).collect())
    }
}

/// Word pool for synthetic filenames. 20 words ⇒ a plain-word query matches
/// roughly 1/20 of the files, the "a common word, lots of hits" shape that makes
/// the ranking phase visible.
const WORDS: [&str; 20] = [
    "report",
    "invoice",
    "photo",
    "screenshot",
    "notes",
    "budget",
    "resume",
    "draft",
    "backup",
    "export",
    "meeting",
    "recipe",
    "receipt",
    "sketch",
    "logo",
    "banner",
    "config",
    "readme",
    "license",
    "changelog",
];

const EXTS: [&str; 8] = ["pdf", "jpg", "png", "txt", "md", "rs", "json", "zip"];

/// Build a synthetic index of roughly `n` entries shaped like a real home dir:
/// a fanned-out tree ~8 levels deep, ~12% directories, mixed sizes and mtimes.
pub fn build_synthetic_index(n: usize) -> SearchIndex {
    const FANOUT: usize = 6;
    const MAX_DEPTH: usize = 8;

    let mut arena = ArenaBuilder::with_capacity(n + 1, n * 20);
    let push = |arena: &mut ArenaBuilder, id, parent_id, name: &str, is_dir, i: usize| {
        let size = if is_dir {
            None
        } else {
            Some((i as u64 * 7919) % 50_000_000)
        };
        arena.push(
            id,
            parent_id,
            name,
            is_dir,
            size,
            Some(1_600_000_000 + (i as u64 * 613) % 60_000_000),
        );
    };

    push(&mut arena, ROOT_ID, 0, "", true, 0);

    // Directories first, breadth-first, until we've spent ~12% of the budget.
    let dir_budget = n / 8;
    let mut next_id: i64 = ROOT_ID + 1;
    let mut level: Vec<i64> = vec![ROOT_ID];
    let mut all_dirs: Vec<i64> = Vec::with_capacity(dir_budget);
    for depth in 0..MAX_DEPTH {
        let mut next_level = Vec::with_capacity(level.len() * FANOUT);
        for &parent in &level {
            for k in 0..FANOUT {
                if all_dirs.len() >= dir_budget {
                    break;
                }
                let name = format!("{}-{}-{}", WORDS[(next_id as usize + k) % WORDS.len()], depth, next_id);
                push(&mut arena, next_id, parent, &name, true, next_id as usize);
                all_dirs.push(next_id);
                next_level.push(next_id);
                next_id += 1;
            }
        }
        if all_dirs.len() >= dir_budget || next_level.is_empty() {
            break;
        }
        level = next_level;
    }

    // Files spread evenly across the directories.
    let mut i = 0usize;
    while arena.len() < n {
        let parent = all_dirs[i % all_dirs.len()];
        let name = format!(
            "{}-{}.{}",
            WORDS[i % WORDS.len()],
            i,
            EXTS[(i / WORDS.len()) % EXTS.len()]
        );
        push(&mut arena, next_id, parent, &name, false, i);
        next_id += 1;
        i += 1;
    }

    // Ids climb in push order, the order the real loader produces (`ORDER BY id`):
    // `index_of_id` binary-searches it.
    arena.finish(1)
}

/// An importance map covering every directory in `index`, like a real
/// `importance-root.db` (a weight on most folders the user has touched).
pub fn synthetic_weights(index: &SearchIndex) -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    for i in 0..index.len() {
        if index.is_directory(i) && index.id(i) != ROOT_ID {
            let path = crate::engine::reconstruct_path_from_index(index, index.id(i));
            weights.insert(&path, ((i % 100) as f64) / 100.0);
        }
    }
    weights
}
