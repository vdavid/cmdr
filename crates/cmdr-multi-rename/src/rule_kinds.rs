//! Which of the sheet's rules a spec actually uses, as a closed set of kinds: what
//! apply hands the host (`MultiRenameHost::rename_applied`), and the desktop reports
//! as `multi_rename_applied`. Categories only, read off the spec's typed shape, never
//! a mask's or a search's text. Crate-private: the host sees only the tokens.

use crate::mask::Mask;
use crate::plan::MultiRenameSpec;
use crate::transform::CaseChange;

/// One rule a rename used. A closed set, in the sheet's order, so a new rule is a
/// compile error in [`RuleKind::token`] rather than a silently uncounted one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleKind {
    /// The name mask is something other than the plain `[N]`.
    NameMask,
    /// The extension mask is something other than the plain `[E]`.
    ExtMask,
    /// A mask numbers the rows (`[C]`).
    Counter,
    /// A mask writes a date or time.
    Date,
    /// A mask reads a file's EXIF or tags.
    MediaMeta,
    /// Search & replace has a search.
    SearchReplace,
    /// The case step changes something.
    Case,
    /// Greek to Latin.
    GreekToLatin,
    /// Remove diacritics.
    RemoveDiacritics,
    /// Normalize Unicode.
    NormalizeUnicode,
}

impl RuleKind {
    /// The analytics token.
    pub(crate) fn token(self) -> &'static str {
        match self {
            Self::NameMask => "name_mask",
            Self::ExtMask => "ext_mask",
            Self::Counter => "counter",
            Self::Date => "date",
            Self::MediaMeta => "media_meta",
            Self::SearchReplace => "search_replace",
            Self::Case => "case",
            Self::GreekToLatin => "greek_to_latin",
            Self::RemoveDiacritics => "remove_diacritics",
            Self::NormalizeUnicode => "normalize_unicode",
        }
    }
}

/// [`rule_kinds`] as their tokens: what apply hands `MultiRenameHost::rename_applied`.
pub(crate) fn rule_tokens(spec: &MultiRenameSpec) -> Vec<&'static str> {
    rule_kinds(spec).into_iter().map(RuleKind::token).collect()
}

/// The rules `spec` uses, in [`RuleKind`]'s order. A mask that doesn't parse adds no
/// placeholder kinds (apply refuses it anyway).
pub(crate) fn rule_kinds(spec: &MultiRenameSpec) -> Vec<RuleKind> {
    let masks: Vec<Mask> = [spec.name_mask.as_str(), spec.extension_mask.as_str()]
        .into_iter()
        .filter_map(|mask| Mask::parse(mask).ok())
        .collect();
    let any_mask = |uses: fn(&Mask) -> bool| masks.iter().any(uses);
    [
        (RuleKind::NameMask, spec.name_mask != "[N]"),
        (RuleKind::ExtMask, spec.extension_mask != "[E]"),
        (RuleKind::Counter, any_mask(Mask::uses_counter)),
        (RuleKind::Date, any_mask(Mask::uses_date)),
        (RuleKind::MediaMeta, any_mask(Mask::uses_media)),
        (RuleKind::SearchReplace, !spec.search.is_empty()),
        (RuleKind::Case, spec.case != CaseChange::Unchanged),
        (RuleKind::GreekToLatin, spec.greek_to_latin),
        (RuleKind::RemoveDiacritics, spec.remove_diacritics),
        (RuleKind::NormalizeUnicode, spec.normalize_unicode),
    ]
    .into_iter()
    .filter_map(|(kind, used)| used.then_some(kind))
    .collect()
}
