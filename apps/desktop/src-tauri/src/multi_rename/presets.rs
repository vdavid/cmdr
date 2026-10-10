//! Saved Multi-Rename settings: named presets, the newest first (TC's F2
//! "Load/save settings" list), and the settings the last sheet closed with. The
//! list machinery is `crate::recents`.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};

use super::mask::fill_counter_defaults;
use super::plan::MultiRenameSpec;

/// How many presets the list keeps.
pub const MAX_PRESETS: usize = 200;

/// The presets list. Loaded at startup; read and written through
/// `crate::commands::multi_rename`.
pub static PRESETS: RecentsFile<MultiRenamePreset> = RecentsFile::new();

/// One saved preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenamePreset {
    pub id: String,
    pub name: String,
    pub spec: MultiRenameSpec,
}

/// A preset as stored. Presets saved before the counter lived in the mask carry
/// the sheet's counter fields; loading folds them into the masks' counters, so
/// they keep producing the same names. Nothing writes them anymore.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPreset {
    id: String,
    name: String,
    spec: StoredSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSpec {
    #[serde(flatten)]
    spec: MultiRenameSpec,
    counter_start: Option<i64>,
    counter_step: Option<i64>,
    counter_digits: Option<u32>,
}

impl<'de> Deserialize<'de> for MultiRenamePreset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let StoredPreset { id, name, spec } = StoredPreset::deserialize(deserializer)?;
        let StoredSpec {
            mut spec,
            counter_start,
            counter_step,
            counter_digits,
        } = spec;
        let fill = |mask: &str| {
            fill_counter_defaults(
                mask,
                counter_start.unwrap_or(1),
                counter_step.unwrap_or(1),
                counter_digits.unwrap_or(1),
            )
        };
        spec.name_mask = fill(&spec.name_mask);
        spec.extension_mask = fill(&spec.extension_mask);
        Ok(Self { id, name, spec })
    }
}

impl RecentEntry for MultiRenamePreset {
    const FILENAME: &'static str = "multi-rename-presets.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename presets";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// A preset is its name: saving under a name that's taken replaces that preset.
    fn dedupe_key(&self) -> String {
        self.name.trim().to_lowercase()
    }
}

/// Renames preset `id` to `name` (trimmed) where it stands in the list. Another
/// preset already called that is dropped: a preset is its name, and the sheet asks
/// before replacing one. Returns whether anything changed: `false` for an unknown
/// id or an empty name.
pub fn rename_in(presets: &mut Vec<MultiRenamePreset>, id: &str, name: &str) -> bool {
    let name = name.trim();
    let Some(at) = presets.iter().position(|p| p.id == id) else {
        return false;
    };
    if name.is_empty() || presets[at].name == name {
        return false;
    }
    presets[at].name = name.to_string();
    let key = presets[at].dedupe_key();
    presets.retain(|p| p.id == id || p.dedupe_key() != key);
    true
}

/// Gives preset `id` these settings where it stands in the list, so updating a
/// preset doesn't renumber the menu. Returns whether anything changed.
pub fn update_spec_in(presets: &mut [MultiRenamePreset], id: &str, spec: &MultiRenameSpec) -> bool {
    match presets.iter_mut().find(|p| p.id == id) {
        Some(preset) if preset.spec != *spec => {
            preset.spec = spec.clone();
            true
        }
        _ => false,
    }
}

/// The settings the sheet last closed with, so the next ⌃M opens where the last
/// one left off (TC keeps them too). One entry, in its own file.
pub static LAST_SPEC: RecentsFile<LastSpec> = RecentsFile::new();

/// The preset the sheet's fields came from, so a reopened sheet's Presets button
/// names it (and says "edited" when the fields moved off it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LoadedPreset {
    Saved {
        id: String,
    },
    /// One that ships with Cmdr (`spec.ts`'s `BUILT_IN_PRESETS`).
    BuiltIn {
        id: String,
    },
}

/// What the sheet closed with: its fields, and the preset they came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameLastSettings {
    pub spec: MultiRenameSpec,
    /// Absent in a file saved before the sheet remembered it.
    #[serde(default)]
    pub preset: Option<LoadedPreset>,
}

/// The one entry `LAST_SPEC` keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSpec {
    pub id: String,
    #[serde(flatten)]
    pub settings: MultiRenameLastSettings,
}

impl LastSpec {
    const ID: &'static str = "last";

    pub fn new(spec: MultiRenameSpec, preset: Option<LoadedPreset>) -> Self {
        Self {
            id: Self::ID.to_string(),
            settings: MultiRenameLastSettings { spec, preset },
        }
    }
}

impl RecentEntry for LastSpec {
    const FILENAME: &'static str = "multi-rename-last.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename last settings";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Always the same entry, so saving replaces it.
    fn dedupe_key(&self) -> String {
        Self::ID.to_string()
    }
}
