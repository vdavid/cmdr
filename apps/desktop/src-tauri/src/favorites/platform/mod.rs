//! Which favorites a first launch seeds, per OS. A platform seam
//! (`docs/guides/platform-seams.md`): one arm per OS, picked below.

use std::path::{Path, PathBuf};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&Path) -> Vec<(PathBuf, &'static str)> = default_seed;

/// The folders a first launch seeds as favorites, in menu order, each with the name its row
/// shows. `home` is the user's own folder.
///
/// The names are this OS's own words for the places (`docs/style-guide.md` § "Platform
/// vocabulary"), in English: a seeded name is written to `favorites.json` once and is the
/// user's to rename from then on, so it never follows the UI language.
pub(super) use imp::default_seed;
