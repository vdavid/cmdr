//! What runs on a name after its mask: search & replace, then the case step, then
//! removing diacritics, in Total Commander's order (the mask first, case last).

use regex::{Regex, RegexBuilder};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Search & replace, as the sheet's fields and switches set it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replace {
    /// Plain text with `*` / `?` wildcards and `a|b` parallel lists, or a regex.
    pub search: String,
    /// The replacement; `x|y` pairs with a list search, `$1` groups with a regex.
    pub replace: String,
    /// TC's `^`. Off: case-insensitive.
    pub case_sensitive: bool,
    /// TC's `1x`: only the first match.
    pub first_only: bool,
    /// TC's `[E]`: the extension too.
    pub include_extension: bool,
    pub regex: bool,
    /// TC's `Subst.`: the whole name becomes the replacement (with its groups)
    /// when the search matches.
    pub substitute: bool,
}

/// The case step, after search & replace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CaseChange {
    #[default]
    Unchanged,
    Lower,
    Upper,
    /// The first letter upper, the rest lower.
    FirstUpper,
    /// Every word's first letter upper, the rest lower.
    Words,
}

/// Everything after the mask.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transform {
    pub replace: Option<Replace>,
    pub case: CaseChange,
    pub remove_diacritics: bool,
}

/// Why a search can't run. Typed, so the frontend words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceError {
    BadRegex { detail: String },
}

impl Transform {
    /// The `(name, extension)` after search & replace, case, and diacritics.
    pub fn apply(&self, name: &str, ext: &str) -> Result<(String, String), ReplaceError> {
        let (mut name, mut ext) = (name.to_string(), ext.to_string());
        if let Some(replace) = self.replace.as_ref().filter(|r| !r.search.is_empty())
            && let Some(rule) = rules(replace)?
        {
            name = run_rules(&rule, &nfc(&name), replace);
            if replace.include_extension {
                ext = run_rules(&rule, &nfc(&ext), replace);
            }
        }
        name = change_case(&name, self.case);
        ext = change_case(&ext, self.case);
        if self.remove_diacritics {
            name = remove_diacritics(&name);
            ext = remove_diacritics(&ext);
        }
        Ok((name, ext))
    }
}

/// The compiled search and what replaces each match.
struct Rule {
    pattern: Regex,
    replacement: Replacement,
}

enum Replacement {
    /// A regex search: the replacement with its `$1` groups.
    Template(String),
    /// A plain search `a|b|c`: one alternation, group `i + 1` is search `i`, and
    /// this is its literal replacement. ONE pass, so `a|b` → `b|c` turns `a` into
    /// `b` (not `c`) and `one|two` → `two|one` swaps, as in Total Commander.
    Pairs(Vec<String>),
}

impl Replacement {
    fn for_match(&self, caps: &regex::Captures<'_>) -> String {
        match self {
            Self::Template(template) => {
                let mut out = String::new();
                caps.expand(template, &mut out);
                out
            }
            Self::Pairs(replacements) => (1..caps.len())
                .find(|&group| caps.get(group).is_some())
                .and_then(|group| replacements.get(group - 1))
                .cloned()
                .unwrap_or_default(),
        }
    }
}

fn rules(replace: &Replace) -> Result<Option<Rule>, ReplaceError> {
    let build = |pattern: &str| {
        RegexBuilder::new(pattern)
            .case_insensitive(!replace.case_sensitive)
            .build()
            .map_err(|err| ReplaceError::BadRegex {
                detail: err.to_string(),
            })
    };
    if replace.regex {
        return Ok(Some(Rule {
            pattern: build(&nfc(&replace.search))?,
            replacement: Replacement::Template(replace.replace.clone()),
        }));
    }
    let search = nfc(&replace.search);
    let searches: Vec<&str> = search.split('|').collect();
    // A list pairs with a list; a single search takes its replacement literally,
    // `|` and all.
    let replacements: Vec<&str> = if searches.len() > 1 {
        replace.replace.split('|').collect()
    } else {
        vec![replace.replace.as_str()]
    };
    let mut alternatives = Vec::new();
    let mut paired = Vec::new();
    for (i, search) in searches.iter().enumerate().filter(|(_, s)| !s.is_empty()) {
        alternatives.push(format!("({})", wildcard_pattern(search)));
        paired.push(match replacements.len() {
            1 => replacements[0].to_string(),
            _ => replacements.get(i).copied().unwrap_or("").to_string(),
        });
    }
    // Nothing but `|`s: there's nothing to search for.
    if alternatives.is_empty() {
        return Ok(None);
    }
    Ok(Some(Rule {
        pattern: build(&alternatives.join("|"))?,
        replacement: Replacement::Pairs(paired),
    }))
}

/// `text` composed (NFC), so a typed `é` finds the decomposed one a name from an
/// SMB share or HFS carries.
fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// A plain search with `*` (any run) and `?` (any one character) as a regex. A
/// `*` stops at the first place the rest matches (`IMG_*_` ends at the first
/// `_`), except a trailing one, which runs to the end (`foo*` is `foo` and all
/// that follows).
fn wildcard_pattern(search: &str) -> String {
    let mut pattern = String::new();
    let last = search.chars().count().saturating_sub(1);
    for (i, c) in search.chars().enumerate() {
        match c {
            '*' if i == last => pattern.push_str(".*"),
            '*' => pattern.push_str(".*?"),
            '?' => pattern.push('.'),
            c => pattern.push_str(&regex::escape(&c.to_string())),
        }
    }
    pattern
}

fn run_rules(rule: &Rule, text: &str, replace: &Replace) -> String {
    if replace.substitute {
        return match rule.pattern.captures(text) {
            Some(caps) => rule.replacement.for_match(&caps),
            None => text.to_string(),
        };
    }
    let limit = if replace.first_only { 1 } else { 0 };
    rule.pattern
        .replacen(text, limit, |caps: &regex::Captures<'_>| {
            rule.replacement.for_match(caps)
        })
        .into_owned()
}

fn change_case(text: &str, case: CaseChange) -> String {
    match case {
        CaseChange::Unchanged => text.to_string(),
        CaseChange::Lower => text.to_lowercase(),
        CaseChange::Upper => text.to_uppercase(),
        CaseChange::FirstUpper => {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.as_str().to_lowercase().chars())
                    .collect(),
                None => String::new(),
            }
        }
        CaseChange::Words => {
            let mut out = String::with_capacity(text.len());
            let mut starts_word = true;
            for c in text.chars() {
                if starts_word {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                starts_word = !c.is_alphanumeric();
            }
            out
        }
    }
}

/// `text` without diacritics on Latin and Greek letters: each such letter is
/// decomposed (NFD) and its combining marks dropped, plus a table for the letters
/// that don't decompose (`ł`, `đ`, `ø`, `ß`, `æ`, `œ`, `þ`, `ı`), like foobar2000's
/// `$ascii()`. Every other script is left exactly as it is: a kana's dakuten, a
/// Devanagari or Thai vowel sign, and Cyrillic `й` / `ё` are part of the letter,
/// not decoration.
pub fn remove_diacritics(text: &str) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.nfc() {
        if let Some(replacement) = undecomposed(c) {
            out.push_str(replacement);
        } else if is_latin_or_greek(c) {
            out.extend(c.to_string().nfd().filter(|m| !is_combining_mark(*m)));
        } else {
            out.push(c);
        }
    }
    out
}

/// Latin (Basic through Extended Additional) and Greek letters: the scripts whose
/// marks are diacritics a name can do without.
fn is_latin_or_greek(c: char) -> bool {
    matches!(c as u32, 0x00C0..=0x024F | 0x1E00..=0x1EFF | 0x0370..=0x03FF | 0x1F00..=0x1FFF)
}

fn undecomposed(c: char) -> Option<&'static str> {
    Some(match c {
        'ł' => "l",
        'Ł' => "L",
        'đ' => "d",
        'Đ' => "D",
        'ø' => "o",
        'Ø' => "O",
        'ß' => "ss",
        'ẞ' => "SS",
        'æ' => "ae",
        'Æ' => "Ae",
        'œ' => "oe",
        'Œ' => "Oe",
        'þ' => "th",
        'Þ' => "Th",
        'ð' => "d",
        'Ð' => "D",
        'ı' => "i",
        'ħ' => "h",
        'Ħ' => "H",
        'ŧ' => "t",
        'Ŧ' => "T",
        _ => return None,
    })
}
