//! What a group got wrong, typed: the one vocabulary every check here refuses in.
//!
//! Its own module so the checks (`input.rs`, `place.rs`, `names.rs`) all depend on it and it
//! depends on none of them: each refuses in this vocabulary, and none needs another's types to
//! do so.

use cmdr_fs::validation::PlainLeafRefusal;

use crate::location::miss::LocationMissResult;

/// The most UTF-16 code units a phone takes in a name: a PTP string's 255 count its
/// terminator. `mtp-rs`'s own number, so a proposal refuses exactly what the device layer
/// would refuse after approval as `VolumeError::InvalidName` (`crates/cmdr-mtp/DETAILS.md`
/// § Names a phone takes). An ADB destination takes the same limit.
pub(super) const PHONE_NAME_UNITS: usize = cmdr_mtp::MAX_NAME_UNITS;

/// What one group got wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GroupProblem {
    /// A server verb (`addServer`, `editServer`): the store's verb vocabulary has them, and a
    /// file sweep can't carry one.
    NotAFileVerb,
    /// Neither `paths`, `renames`, nor `selector`.
    NoSources,
    /// More than one of them: which one the user would be reviewing is a coin toss.
    AmbiguousSources,
    /// `renames` under a verb that can't take names (extract, compress, trash, delete), or
    /// `rename` without them.
    RenamesVerbMismatch,
    /// A selector under `rename`.
    SelectorCantRename,
    /// A destination a ROUTE serves, which takes no writes: inside an archive, or inside
    /// a repo's virtual `.git` trees. Copy or move INTO a zip, and move OUT of one, plan
    /// from their own walk rather than the per-source engine, so a group written there
    /// could not be held to the sources the user reviewed; a snapshot has no file to
    /// write to at all. Refused HERE, where it costs the model a retry, rather than at
    /// execution where it would refuse after the user had already approved.
    DestinationTakesNoWrites,
    /// A field the verb's executor doesn't bind (a destination on a trash group, a parent
    /// on a move).
    UnboundField {
        field: &'static str,
    },
    /// A field the verb's executor requires.
    MissingField {
        field: &'static str,
    },
    /// `sourceVolumeId` / `displayName` sent alongside a selector, which supplies both.
    SelectorSuppliesField {
        field: &'static str,
    },
    EmptySources,
    TooManyPaths {
        sent: usize,
    },
    /// A path that is neither absolute nor a `scheme://` virtual path.
    RelativePath {
        path: String,
    },
    /// A new name that isn't one plain name, and the rule it breaks.
    NotABareName {
        name: String,
        rule: NameRule,
    },
    /// Two files of a move or copy would land under one name, ignoring case and Unicode form.
    DuplicateLeaf {
        name: String,
    },
    /// A file of a move or copy would land where another of its sources is.
    ChainOverwrite {
        path: String,
    },
    /// A move or copy with names whose destination already holds one of its sources: that's a
    /// rename for those files.
    UseRename,
    /// Two sources of a move or copy share an original name on a route that keys each landing
    /// on it (both ends local), so its start would refuse them, named apart or not.
    SharedName {
        name: String,
        first: String,
        second: String,
    },
    /// A size or age window nothing can satisfy.
    ImpossibleWindow,
    /// A place the resolver couldn't put on a live volume, nor re-address onto a live sibling.
    Location {
        field: PlaceField,
        miss: Box<LocationMissResult>,
    },
    /// Re-addressing would take the group onto two members of a share, or re-address one
    /// place while another names a member of that share directly: one `via` can't say it.
    MixedReaddress,
}

/// Which field of a group a place came from, so a refusal says which one to fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaceField {
    Destination,
    SelectorRoot,
    Paths,
    Renames,
    Parent,
}

impl PlaceField {
    /// The field's name as the model wrote it.
    pub(super) fn token(self) -> &'static str {
        match self {
            PlaceField::Destination => "destination",
            PlaceField::SelectorRoot => "selector.root",
            PlaceField::Paths => "paths",
            PlaceField::Renames => "renames",
            PlaceField::Parent => "parent",
        }
    }
}

/// Which rule a proposed name breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameRule {
    /// One of [`cmdr_fs::validation::validate_plain_leaf`]'s.
    Plain(PlainLeafRefusal),
    /// Past [`PHONE_NAME_UNITS`] on a phone.
    PhoneTooLong,
    /// One of the characters a phone reserves (`names.rs`'s `PHONE_RESERVED`).
    PhoneReserved(char),
}

impl NameRule {
    /// What's wrong with the name, as the end of a sentence the model reads.
    pub(super) fn reason(self) -> String {
        match self {
            NameRule::Plain(PlainLeafRefusal::Empty) => "it's empty".into(),
            NameRule::Plain(PlainLeafRefusal::EdgeWhitespace) => "it starts or ends with a space".into(),
            NameRule::Plain(PlainLeafRefusal::Separator) => {
                "it holds a /, so it's a path; give the bare name it lands as".into()
            }
            NameRule::Plain(PlainLeafRefusal::DotName) => "it's . or .., which name a folder".into(),
            NameRule::Plain(PlainLeafRefusal::TooLong { bytes }) => {
                format!("it's longer than the 255 bytes a file name may be ({bytes})")
            }
            NameRule::Plain(PlainLeafRefusal::Control(ch)) => {
                format!("it holds the control character U+{:04X}", u32::from(ch))
            }
            NameRule::Plain(PlainLeafRefusal::Invisible(ch)) => format!(
                "it holds the invisible character U+{:04X}, which would make it read as something else",
                u32::from(ch)
            ),
            NameRule::PhoneTooLong => {
                format!("it's longer than the {PHONE_NAME_UNITS} UTF-16 characters a phone takes in a name")
            }
            NameRule::PhoneReserved(ch) => format!("a phone refuses {ch} in a name"),
        }
    }
}
