//! An entry's full path, read off the arena's `parent_id` chain: built as a string for
//! a result row, or streamed into a hash for a ranking weight lookup.
//!
//! Arena-shaped and nothing else, so both the scan (`engine.rs`) and ranking
//! (`ranking.rs`) read it from here, below either of them.

use super::{ARENA_ROOT_ID, SearchIndex};

/// Reconstruct the full path for an entry by walking the parent_id chain
/// in the in-memory index. O(depth) per entry.
pub fn reconstruct_path_from_index(index: &SearchIndex, entry_id: i64) -> String {
    if entry_id == ARENA_ROOT_ID {
        return "/".to_string();
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ARENA_ROOT_ID || current_id == 0 {
            break;
        }
        match index.index_of_id(current_id) {
            Some(idx) => {
                let name = index.name(idx);
                if name.is_empty() {
                    break; // root sentinel
                }
                components.push(name);
                current_id = index.parent_id(idx);
            }
            None => break, // orphan or missing parent
        }
    }

    components.reverse();
    format!("/{}", components.join("/"))
}

/// The [`hash_path`](cmdr_fs::path_hash::hash_path) of an entry's full path, without ever
/// building that path.
///
/// The ranking blend needs a folder's importance weight, and a weight lookup is a
/// hash lookup — the path `String` [`reconstruct_path_from_index`] would build exists
/// only to be hashed and dropped. A broad query ranks millions of candidates, so this
/// walks the same parent chain and streams the bytes into a [`PathHasher`](cmdr_fs::path_hash::PathHasher) instead.
/// Byte-identical to hashing the reconstructed path (pinned by
/// `streamed_hash_matches_whole_path_hash`).
pub(crate) fn hash_path_from_index(index: &SearchIndex, entry_id: i64) -> u64 {
    let mut hasher = cmdr_fs::path_hash::PathHasher::new();
    if entry_id == ARENA_ROOT_ID {
        hasher.write(b"/");
        return hasher.finish();
    }

    // The chain yields components leaf-first; the path needs them root-first, so
    // collect the borrowed names before hashing. On the STACK: ranking calls this
    // once per folder a broad query's matches sit in (hundreds of thousands), and a
    // `Vec` here was an allocation each. A chain deeper than the stack buffer spills
    // its root-side remainder to `deeper`, which allocates only then.
    const INLINE_DEPTH: usize = 64;
    let mut inline: [&str; INLINE_DEPTH] = [""; INLINE_DEPTH];
    let mut deeper: Vec<&str> = Vec::new();
    let mut depth = 0;
    let mut current_id = entry_id;
    loop {
        if current_id == ARENA_ROOT_ID || current_id == 0 {
            break;
        }
        match index.index_of_id(current_id) {
            Some(idx) => {
                let name = index.name(idx);
                if name.is_empty() {
                    break; // root sentinel
                }
                if depth < INLINE_DEPTH {
                    inline[depth] = name;
                } else {
                    deeper.push(name);
                }
                depth += 1;
                current_id = index.parent_id(idx);
            }
            None => break, // orphan or missing parent
        }
    }

    if depth == 0 {
        // `format!("/{}", "")` — the reconstructed path for an empty chain.
        hasher.write(b"/");
        return hasher.finish();
    }
    // Root-first: the spilled root-side names, then the inline leaf-side ones.
    for name in deeper
        .iter()
        .rev()
        .chain(inline[..depth.min(INLINE_DEPTH)].iter().rev())
    {
        hasher.write(b"/");
        hasher.write(name.as_bytes());
    }
    hasher.finish()
}
