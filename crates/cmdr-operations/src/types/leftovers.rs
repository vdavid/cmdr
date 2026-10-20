//! What a stopped write left at its destination, by fate: Cmdr's own scratch a
//! stop's cleanup couldn't take at once, and the complete new files a landing
//! kept. A leaf of its own, because a cancel's summary (`events.rs`) and a
//! failure (`errors.rs`, `WriteOperationError::NewFilesKept`) both carry it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One group of Cmdr's own unfinished `.cmdr-tmp-*` writes that a stop's
/// cleanup couldn't take off the destination at once, all with one fate.
///
/// **Deliberately not a `SkipBreakdown`.** A skip is a LEDGER item the
/// reversal walked to and chose to leave alone, named by the file the user is
/// looking at; these are Cmdr's own scratch files for writes that never
/// finished, and nothing chose to keep them (`transfer/volume/cleanup.rs`).
/// Folding them into `skips` would also fold them into `reversed + skipped`,
/// which is what the reversal's progress bar drains over, so the bar would
/// report progress across items no reversal ever walks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StagedLeftovers {
    /// What becomes of them, which is what the line says.
    pub fate: StagedLeftoverFate,
    /// Every one with this fate, not a sample.
    pub count: u32,
    /// The leaf name of the FIRST one, so a report can name it when it's the
    /// only one: what the user would look for at the destination. A removable
    /// temp's own scratch name (`photo.jpg.cmdr-tmp-<uuid>`); for a
    /// [`StagedLeftoverFate::LandsWhenBack`], the name the file is on its way
    /// to (`photo.jpg`); for a [`StagedLeftoverFate::Rescued`], the name it
    /// sits under now (`photo (recovered).jpg`).
    pub example_name: String,
    /// The destination's name as it names itself, so the line can say where
    /// "back" is.
    pub volume_name: String,
}

/// What becomes of a [`StagedLeftovers`] group.
///
/// ❗ Typed, so the readout words each one without reading a path or a message:
/// a removable scratch file and a COMPLETE file waiting for its name are
/// different news, and only the second is the user's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StagedLeftoverFate {
    /// The destination answered and refused the delete (an abandoned write's
    /// handle can hold the file open for as long as its session lives). Cmdr
    /// tries once more a few seconds later, then the next time it connects to
    /// the destination.
    Refused,
    /// The destination was out. Cmdr removes them as soon as it's back.
    RemovedWhenBack,
    /// Complete files on their way to their names when the operation stopped,
    /// after the way there may already have been cleared: possibly the only
    /// copy at the destination, ❌ never junk. Each gets its name the next time
    /// Cmdr connects to the destination.
    LandsWhenBack,
    /// Complete files that couldn't take their names after the way there was
    /// cleared, so they left Cmdr's scratch space under a ` (recovered)` name:
    /// possibly the only copy, ❌ never junk. Named by that name, which is the
    /// file the person goes to look at.
    Rescued,
}

impl StagedLeftoverFate {
    /// Whether this fate is the person's new data rather than Cmdr's scratch:
    /// what a failure names beside its cause (`WriteOperationError::NewFilesKept`).
    pub const fn holds_new_data(self) -> bool {
        matches!(self, Self::LandsWhenBack | Self::Rescued)
    }
}

impl StagedLeftovers {
    /// One group of `paths` with `fate`, on `volume_name`. `None` for no paths.
    pub fn of(fate: StagedLeftoverFate, paths: &[PathBuf], volume_name: &str) -> Option<Self> {
        let first = paths.first()?;
        Some(Self {
            fate,
            count: paths.len() as u32,
            example_name: first
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            volume_name: volume_name.to_owned(),
        })
    }
}
