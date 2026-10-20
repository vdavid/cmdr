//! How much "Delete all Cmdr data" would free, for the dialog's honest numbers. Reads sizes
//! only; never follows a symlink out of a target.

use std::path::Path;

use super::plan::WipeTarget;

/// The local AI model's folder and file extension, under the data dir (`ai/state.rs`).
const AI_DIR: &str = "ai";
const AI_MODEL_EXTENSION: &str = "gguf";
/// The image-search (CLIP) model's folder, under the data dir (`cmdr-index`'s
/// `media_index::clip::install::clip_model_dir`).
const CLIP_MODEL_DIR: &str = "clip-model";

/// Sizes in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sizes {
    /// Everything the plan would remove.
    pub total: u64,
    /// The downloaded AI models among it: the local LLM and the image-search model.
    pub models: u64,
}

pub fn measure(plan: &[WipeTarget], data_dir: &Path) -> Sizes {
    let total = plan.iter().map(|target| size_of(&target.path)).sum();
    Sizes {
        total,
        models: model_bytes(data_dir),
    }
}

fn model_bytes(data_dir: &Path) -> u64 {
    let llm: u64 = std::fs::read_dir(data_dir.join(AI_DIR))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|ext| ext == AI_MODEL_EXTENSION))
                .map(|e| size_of(&e.path()))
                .sum()
        })
        .unwrap_or(0);
    llm + size_of(&data_dir.join(CLIP_MODEL_DIR))
}

/// A file's length, or a folder's files added up. A symlink counts as itself (nothing).
fn size_of(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| size_of(&e.path())).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_wipe::plan::TargetKind;
    use crate::test_support::TestDir;

    #[test]
    fn totals_every_target_and_picks_out_the_models() {
        let root = TestDir::new("wipe-sizes");
        let data = root.join("data");
        std::fs::create_dir_all(data.join("ai").join("memory")).unwrap();
        std::fs::write(data.join("ai").join("model.gguf"), vec![0u8; 1000]).unwrap();
        std::fs::write(data.join("ai").join("memory").join("note.md"), vec![0u8; 10]).unwrap();
        std::fs::create_dir_all(data.join("clip-model").join("tower")).unwrap();
        std::fs::write(data.join("clip-model").join("tower").join("weights"), vec![0u8; 500]).unwrap();
        std::fs::write(data.join("settings.json"), vec![0u8; 5]).unwrap();
        let cache = root.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("index.db"), vec![0u8; 2000]).unwrap();
        let plan = vec![
            WipeTarget {
                kind: TargetKind::AppData,
                path: data.clone(),
            },
            WipeTarget {
                kind: TargetKind::Cache,
                path: cache,
            },
            WipeTarget {
                kind: TargetKind::WebKit,
                path: root.join("missing"),
            },
        ];

        assert_eq!(
            measure(&plan, &data),
            Sizes {
                total: 1000 + 10 + 500 + 5 + 2000,
                models: 1500
            }
        );
    }
}
