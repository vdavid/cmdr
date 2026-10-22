//! Saved Multi-Rename settings: named presets in the user's order (TC's F2
//! "Load/save settings" list), and the settings the last sheet closed with. The
//! presets file is `preset_store`, which builds on this module and never the other
//! way around; the last settings ride on `crate::recents`.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};

use super::mask::fill_counter_defaults;
use super::plan::MultiRenameSpec;

/// How many presets the list holds; saving a new one past it is refused.
pub const MAX_PRESETS: usize = 200;

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

impl MultiRenamePreset {
    /// A preset is its name, trimmed and in any case: saving under a name that's
    /// taken replaces that preset.
    fn key(&self) -> String {
        name_key(&self.name)
    }
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Why a save didn't change the list. Typed, so the sheet words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PresetSaveRefusal {
    /// The list holds `max` presets already, and this name is a new one.
    ListFull { max: u32 },
}

/// Saves `preset`: over the preset with its name, where that one stands, or as a
/// new one at the end, so no other preset's number moves. A new one past `max` is
/// refused, and the list stays as it was.
pub fn save_in(
    presets: &mut Vec<MultiRenamePreset>,
    mut preset: MultiRenamePreset,
    max: usize,
) -> Result<(), PresetSaveRefusal> {
    let key = preset.key();
    if let Some(at) = presets.iter().position(|p| p.key() == key) {
        presets[at] = preset;
        // A list that somehow grew two presets with one name heals here.
        let id = presets[at].id.clone();
        presets.retain(|p| p.id == id || p.key() != key);
        return Ok(());
    }
    if presets.len() >= max {
        return Err(PresetSaveRefusal::ListFull {
            max: u32::try_from(max).unwrap_or(u32::MAX),
        });
    }
    // Keep ids unique even when the caller hands over one the list holds.
    if presets.iter().any(|p| p.id == preset.id) {
        preset.id = uuid::Uuid::new_v4().to_string();
    }
    presets.push(preset);
    Ok(())
}

/// Which way Move up / Move down takes a preset in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PresetMove {
    Up,
    Down,
}

/// Swaps preset `id` with its neighbor above or below. Returns whether it moved:
/// `false` at that end of the list, or for an unknown id.
pub fn move_in(presets: &mut [MultiRenamePreset], id: &str, direction: PresetMove) -> bool {
    let Some(at) = presets.iter().position(|p| p.id == id) else {
        return false;
    };
    let to = match direction {
        PresetMove::Up => at.checked_sub(1),
        PresetMove::Down => Some(at + 1).filter(|&to| to < presets.len()),
    };
    let Some(to) = to else {
        return false;
    };
    presets.swap(at, to);
    true
}

/// Deletes preset `id`; the rest keep their order. Returns whether it was there.
pub fn delete_in(presets: &mut Vec<MultiRenamePreset>, id: &str) -> bool {
    let before = presets.len();
    presets.retain(|p| p.id != id);
    presets.len() != before
}

/// The presets, top to bottom, and whether this list ever took the seeds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetList {
    pub entries: Vec<MultiRenamePreset>,
    /// The seeds went in (or the list's file was set aside, which isn't a new
    /// install). Never goes back to `false`, so a deleted seed stays deleted.
    pub seeded: bool,
}

/// A preset Cmdr starts the list with ("Date and time", "Remove diacritics", …),
/// named in the UI language: the frontend owns the specs and the words, the
/// backend decides whether they go in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresetSeed {
    pub name: String,
    pub spec: MultiRenameSpec,
}

/// Adds the seeds after the user's presets, once per list: none whose name the
/// list already has, none past `max`. Returns whether it ran.
pub fn seed_in(list: &mut PresetList, seeds: Vec<PresetSeed>, max: usize) -> bool {
    if list.seeded {
        return false;
    }
    for PresetSeed { name, spec } in seeds {
        let key = name_key(&name);
        if key.is_empty() || list.entries.len() >= max || list.entries.iter().any(|p| p.key() == key) {
            continue;
        }
        list.entries.push(MultiRenamePreset {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            spec,
        });
    }
    list.seeded = true;
    true
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
    let key = presets[at].key();
    presets.retain(|p| p.id == id || p.key() != key);
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

/// What the sheet closed with: its fields, and the preset they came from, so a
/// reopened sheet's Presets button names it (and says "edited" when the fields
/// moved off it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameLastSettings {
    pub spec: MultiRenameSpec,
    pub preset_id: Option<String>,
}

/// The one entry `LAST_SPEC` keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "StoredLastSpec")]
pub struct LastSpec {
    pub id: String,
    #[serde(flatten)]
    pub settings: MultiRenameLastSettings,
}

impl LastSpec {
    const ID: &'static str = "last";

    pub fn new(spec: MultiRenameSpec, preset_id: Option<String>) -> Self {
        Self {
            id: Self::ID.to_string(),
            settings: MultiRenameLastSettings { spec, preset_id },
        }
    }
}

/// `LastSpec` as stored, by this build or an older one: a file saved before the
/// sheet remembered its preset has none, and one saved while Cmdr's own presets
/// were a separate kind names the preset as `preset: { kind, id }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLastSpec {
    id: String,
    spec: MultiRenameSpec,
    #[serde(default)]
    preset_id: Option<String>,
    #[serde(default)]
    preset: Option<OlderLoadedPreset>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum OlderLoadedPreset {
    Saved {
        id: String,
    },
    /// One that shipped with Cmdr. Those are seeded presets now, with ids of their
    /// own, so the fields stay and no preset is named.
    BuiltIn {},
}

impl From<StoredLastSpec> for LastSpec {
    fn from(stored: StoredLastSpec) -> Self {
        let older = match stored.preset {
            Some(OlderLoadedPreset::Saved { id }) => Some(id),
            Some(OlderLoadedPreset::BuiltIn {}) | None => None,
        };
        Self {
            id: stored.id,
            settings: MultiRenameLastSettings {
                spec: stored.spec,
                preset_id: stored.preset_id.or(older),
            },
        }
    }
}

impl RecentEntry for LastSpec {
    const FILENAME: &'static str = "multi-rename-last.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename last settings";
    const STATE_FILE: crate::state_recovery::StateFile = crate::state_recovery::StateFile::MultiRenameLastSettings;

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
