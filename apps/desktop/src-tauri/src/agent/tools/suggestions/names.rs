//! The names a proposal gives its files: a rename's, and a move's or copy's landing under a
//! new one.
//!
//! Every rule here is the write engine's own, asked ahead of time: a plain leaf
//! (`cmdr_fs::validation::validate_plain_leaf`), and for a move or copy the landing rules its
//! start enforces (`TransferLeaves::check_landings`: distinct ignoring case and Unicode
//! form, nothing landing on another source, no name into a source's own folder). ❌ Never a
//! second copy of either: a proposal that passes here and refuses at the start costs the user
//! an approval that does nothing. The one rule the engine doesn't hold is a phone's, so it
//! lives here alone. The rules and the why: `DETAILS.md` § Moving or copying under a new name.

use std::path::Path;

use cmdr_fs::validation::validate_plain_leaf;
use cmdr_operations::{LandingClash, TransferLeaves};

use super::problem::{GroupProblem, NameRule, PHONE_NAME_UNITS};
use crate::agent::store::proposals::TransferOp;
use crate::file_system::volume::Volume;
use crate::mcp::resources::volumes::VolumeKind;

/// The characters Android's shared storage refuses in a name, FAT's reserved set.
const PHONE_RESERVED: [char; 8] = ['"', '*', ':', '<', '>', '?', '\\', '|'];

/// A rename's new name, or the refusal saying which rule it breaks.
pub(super) fn plain_name(name: &str) -> Result<(), GroupProblem> {
    validate_plain_leaf(name).map_err(|rule| not_a_bare_name(name, NameRule::Plain(rule)))
}

/// Normalize a move's or copy's names, then check them the way its start will. `ops` are
/// PLACED (re-addressed and rebased), `destination` is the placed folder, and
/// `destination_kind` its volume's kind.
///
/// A name equal to the source's own becomes `None` first, because "list every file, give a
/// file its current name to keep it" is the contract and the review should draw an arrow only
/// where a name changes. A group whose every name normalizes away is a plain move or copy and
/// gets no name checks, the same as one sent as `paths`.
pub(super) fn check_transfer_names(
    ops: &mut [TransferOp],
    destination: &str,
    destination_kind: Option<VolumeKind>,
) -> Result<(), GroupProblem> {
    for op in ops.iter_mut() {
        let own = Path::new(&op.source_path).file_name().and_then(|name| name.to_str());
        if op.new_name.as_deref() == own {
            op.new_name = None;
        }
    }
    if ops.iter().all(|op| op.new_name.is_none()) {
        return Ok(());
    }

    let items: Vec<(&Path, Option<&str>)> = ops
        .iter()
        .map(|op| (Path::new(&op.source_path), op.new_name.as_deref()))
        .collect();
    TransferLeaves::check_landings(&items, Path::new(destination)).map_err(problem_of)?;

    if matches!(destination_kind, Some(VolumeKind::Mtp | VolumeKind::Adb)) {
        for name in ops.iter().filter_map(|op| op.new_name.as_deref()) {
            phone_name(name)?;
        }
    }
    Ok(())
}

/// Refuse a move's or copy's `sources` when its route can't tell two of them apart: the
/// engine's own ahead-of-start answer (`TransferLeaves::check_own_names`), on the live source
/// and destination volumes, since which engine runs is theirs to say.
pub(super) fn own_names(
    sources: &[&Path],
    source_volume: &dyn Volume,
    dest_volume: &dyn Volume,
) -> Result<(), GroupProblem> {
    TransferLeaves::check_own_names(sources, source_volume, dest_volume).map_err(problem_of)
}

/// The engine's landing clash in the proposal's refusal vocabulary.
fn problem_of(clash: LandingClash) -> GroupProblem {
    match clash {
        LandingClash::SharedOwnName { name, first, second } => GroupProblem::SharedName {
            name,
            first: first.display().to_string(),
            second: second.display().to_string(),
        },
        LandingClash::NotPlain { leaf, refusal } => not_a_bare_name(&leaf, NameRule::Plain(refusal)),
        LandingClash::SameName { landing } => GroupProblem::DuplicateLeaf { name: leaf(&landing) },
        LandingClash::OntoASource { landing } => GroupProblem::ChainOverwrite {
            path: landing.display().to_string(),
        },
        LandingClash::AlreadyInDestination { .. } => GroupProblem::UseRename,
    }
}

/// What a phone takes in a name beyond a plain leaf, refused before the user approves. Below,
/// only the length has a typed backstop, at run time (`crates/cmdr-mtp/DETAILS.md` § Names a
/// phone takes).
fn phone_name(name: &str) -> Result<(), GroupProblem> {
    if name.encode_utf16().count() > PHONE_NAME_UNITS {
        return Err(not_a_bare_name(name, NameRule::PhoneTooLong));
    }
    if let Some(ch) = name.chars().find(|ch| PHONE_RESERVED.contains(ch)) {
        return Err(not_a_bare_name(name, NameRule::PhoneReserved(ch)));
    }
    Ok(())
}

fn not_a_bare_name(name: &str, rule: NameRule) -> GroupProblem {
    GroupProblem::NotABareName {
        name: name.to_string(),
        rule,
    }
}

/// The last component of a landing, which is the name the model chose.
fn leaf(landing: &Path) -> String {
    landing
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
