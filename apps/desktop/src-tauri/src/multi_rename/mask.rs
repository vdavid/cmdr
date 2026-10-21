//! The rename mask: Total Commander's Multi-Rename placeholders, parsed once and
//! rendered per row.
//!
//! Plain text stays as typed; a placeholder sits in brackets. Positions count
//! from 1, and a negative one counts from the end, as in TC:
//!
//! - `[N]` name without extension, `[E]` extension, `[P]` parent folder, `[G]`
//!   grandparent, `[2-5]` the full name. Each takes a range: `[N1]` one
//!   character, `[N2-5]` 2 to 5, `[N2,5]` five from 2, `[N2-]` from 2 to the end,
//!   `[N-8,5]` five from the 8th-last, `[N-8-5]` 8th-last to 5th-last, `[N2--5]`
//!   2 to 5th-last, `[N-5-]` from the 5th-last.
//! - `[C]` the counter, 1, 2, 3 by default (no padding). Each counter carries its
//!   own start, `+`/`-` step, and `:` digits: `[C10+5:3]`, `[C10]`, `[C+5]`,
//!   `[C:3]`, `[C100-10]`. A lone leading sign is the step (`[C-5]` counts down
//!   by five), so a negative start says its step too: `[C-5+1]`.
//! - Date and time of the last modification, local time: `[Y]` `[y]` `[M]` `[D]`
//!   `[h]` `[m]` `[s]`, combined as `[YMD]` or `[hms]`, with `-` `.` `_` or a
//!   space kept between the letters: `[Y-M-D h.m.s]` is `2026-06-15 23.10.09`.
//!   `[d]` is `2026-06-15` and `[t]` is `23.10.09` (no colons, which macOS shows
//!   as slashes).
//! - `[U]` `[L]` `[F]` `[n]`: upper / lower / first letter of each word / the
//!   original case, from that point on.
//! - `[[` is a literal `[`; a `]` outside a placeholder is itself.
//!
//! Characters are counted, never bytes, and a range past the end is empty
//! rather than an error.

use std::fmt::Write as _;

use chrono::{Datelike, NaiveDateTime, Timelike};

/// The widest counter: `[C:4000000000]` must not allocate gigabytes per row.
/// Any name longer than this hits the filesystem's name limit anyway.
pub const MAX_COUNTER_DIGITS: u32 = 64;

/// What a mask knows about one row.
#[derive(Debug, Clone, Copy)]
pub struct RowFacts<'a> {
    /// The full name on disk, extension included.
    pub file_name: &'a str,
    /// A folder has no extension: its whole name is `[N]`.
    pub is_directory: bool,
    pub parent: &'a str,
    pub grandparent: &'a str,
    /// Last modification, local time.
    pub modified: Option<NaiveDateTime>,
    /// The row's place in the rename order, from 0: what the counter counts.
    pub position: usize,
}

impl<'a> RowFacts<'a> {
    /// `(name, extension)`. A leading dot belongs to the name (`.bashrc` has no
    /// extension), a trailing one too (`a.` stays `a.`), and a folder has none.
    pub fn split_name(&self) -> (&'a str, &'a str) {
        if self.is_directory {
            return (self.file_name, "");
        }
        match self.file_name.rfind('.') {
            Some(dot) if dot > 0 && dot + 1 < self.file_name.len() => {
                (&self.file_name[..dot], &self.file_name[dot + 1..])
            }
            _ => (self.file_name, ""),
        }
    }
}

/// Why a mask doesn't parse. Typed, so the frontend words it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MaskError {
    /// A `[` with no `]`; `at` is its character position, from 0.
    Unclosed { at: usize },
    /// A placeholder Cmdr doesn't know, as typed between the brackets.
    Unknown { placeholder: String },
}

/// A parsed mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask(Vec<Token>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Literal(String),
    Field(Field, Range),
    Counter(CounterParts),
    Date(Vec<DatePart>),
    Case(Case),
}

/// A counter's own settings, as typed: `None` is the default (start 1, step 1,
/// one digit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CounterParts {
    start: Option<i64>,
    step: Option<i64>,
    digits: Option<u32>,
}

impl CounterParts {
    /// The text between the brackets, `C` included.
    fn text(&self) -> String {
        let mut out = String::from("C");
        if let Some(start) = self.start {
            let _ = write!(out, "{start}");
        }
        // A negative start alone would read as a step.
        let step = match (self.start, self.step) {
            (Some(start), None) if start < 0 => Some(1),
            (_, step) => step,
        };
        if let Some(step) = step {
            let _ = write!(out, "{step:+}");
        }
        if let Some(digits) = self.digits {
            let _ = write!(out, ":{digits}");
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Extension,
    Parent,
    Grandparent,
    FullName,
}

impl Field {
    /// The text this field reads for `row`.
    fn source<'a>(self, row: &RowFacts<'a>) -> &'a str {
        match self {
            Self::Name => row.split_name().0,
            Self::Extension => row.split_name().1,
            Self::Parent => row.parent,
            Self::Grandparent => row.grandparent,
            Self::FullName => row.file_name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Range {
    All,
    One(i64),
    From(i64),
    Length(i64, usize),
    Span(i64, i64),
}

/// One range placeholder on its own (`N2-5`, `E1`, `-3-`): the field it reads and
/// the characters it takes. For the sheet's range editor, which shows what a range
/// takes from real files without slicing anything itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeToken {
    field: Field,
    range: Range,
}

impl RangeToken {
    /// The range `inner` (a placeholder's text between its brackets) spells, or
    /// `None` for any other placeholder or none.
    pub fn parse(inner: &str) -> Option<Self> {
        match placeholder(inner) {
            Ok(Token::Field(field, range)) => Some(Self { field, range }),
            _ => None,
        }
    }

    /// The text it reads for `row`.
    pub fn source<'a>(&self, row: &RowFacts<'a>) -> &'a str {
        self.field.source(row)
    }

    /// The characters it takes from a text `len` characters long: the first and
    /// last index, from 0, both included; `None` when it takes nothing.
    pub fn bounds(&self, len: usize) -> Option<(usize, usize)> {
        bounds(self.range, len)
    }
}

/// A range as the shared vectors write it: field, kind, `at`, `to`, `length`.
#[cfg(test)]
pub(super) type RangeParts = (&'static str, &'static str, Option<i64>, Option<i64>, Option<u64>);

/// What a whole range token (`[N2-5]`) spells, as the shared vectors write it.
/// For the tests that hold the frontend's parser to this one.
#[cfg(test)]
pub(super) fn range_token(token: &str) -> Option<RangeParts> {
    let inner = token.strip_prefix('[')?.strip_suffix(']')?;
    let Ok(Token::Field(field, range)) = placeholder(inner) else {
        return None;
    };
    let field = match field {
        Field::Name => "name",
        Field::Extension => "extension",
        Field::Parent => "parent",
        Field::Grandparent => "grandparent",
        Field::FullName => "fullName",
    };
    Some(match range {
        Range::All => (field, "all", None, None, None),
        Range::One(at) => (field, "one", Some(at), None, None),
        Range::From(at) => (field, "from", Some(at), None, None),
        Range::Length(at, length) => (field, "length", Some(at), None, u64::try_from(length).ok()),
        Range::Span(at, to) => (field, "span", Some(at), Some(to), None),
    })
}

/// The characters a whole range token (`[N2-5]`) takes from a text `len`
/// characters long, for the tests that hold the frontend's tokens to this one.
#[cfg(test)]
pub(super) fn range_token_bounds(token: &str, len: usize) -> Option<(usize, usize)> {
    let inner = token.strip_prefix('[')?.strip_suffix(']')?;
    let Ok(Token::Field(_, range)) = placeholder(inner) else {
        return None;
    };
    bounds(range, len)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatePart {
    Year,
    ShortYear,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    IsoDate,
    DottedTime,
    /// A `-`, `.`, `_`, or space between the parts, kept as typed.
    Separator(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Upper,
    Lower,
    Words,
    Original,
}

impl Mask {
    pub fn parse(mask: &str) -> Result<Self, MaskError> {
        let chars: Vec<char> = mask.chars().collect();
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '[' if chars.get(i + 1) == Some(&'[') => {
                    literal.push('[');
                    i += 2;
                }
                '[' => {
                    let close = chars[i + 1..]
                        .iter()
                        .position(|&c| c == ']')
                        .ok_or(MaskError::Unclosed { at: i })?;
                    let inner: String = chars[i + 1..i + 1 + close].iter().collect();
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    tokens.push(placeholder(&inner)?);
                    i += close + 2;
                }
                c => {
                    literal.push(c);
                    i += 1;
                }
            }
        }
        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }
        Ok(Self(tokens))
    }

    /// The mask's text for `row`.
    pub fn render(&self, row: &RowFacts<'_>) -> String {
        let mut out = CaseWriter::default();
        for token in &self.0 {
            match token {
                Token::Literal(text) => out.push(text),
                Token::Field(field, range) => out.push(&slice(field.source(row), *range)),
                Token::Counter(CounterParts { start, step, digits }) => {
                    let position = i64::try_from(row.position).unwrap_or(i64::MAX);
                    let value = start
                        .unwrap_or(1)
                        .saturating_add(step.unwrap_or(1).saturating_mul(position));
                    out.push(&pad(value, digits.unwrap_or(1)));
                }
                Token::Date(parts) => {
                    if let Some(when) = row.modified {
                        out.push(&date(&when, parts));
                    }
                }
                Token::Case(case) => out.case = *case,
            }
        }
        out.text
    }
}

fn placeholder(inner: &str) -> Result<Token, MaskError> {
    let unknown = || MaskError::Unknown {
        placeholder: inner.to_string(),
    };
    let mut chars = inner.chars();
    let first = chars.next().ok_or_else(unknown)?;
    let rest = chars.as_str();
    match (first, rest) {
        ('U', "") => return Ok(Token::Case(Case::Upper)),
        ('L', "") => return Ok(Token::Case(Case::Lower)),
        ('F', "") => return Ok(Token::Case(Case::Words)),
        ('n', "") => return Ok(Token::Case(Case::Original)),
        _ => {}
    }
    let field = match first {
        'N' => Some(Field::Name),
        'E' => Some(Field::Extension),
        'P' => Some(Field::Parent),
        'G' => Some(Field::Grandparent),
        _ => None,
    };
    if let Some(field) = field {
        return range(rest).map(|r| Token::Field(field, r)).ok_or_else(unknown);
    }
    if first == 'C' {
        return counter(rest).map(Token::Counter).ok_or_else(unknown);
    }
    if first.is_ascii_digit() || first == '-' {
        return range(inner)
            .map(|r| Token::Field(Field::FullName, r))
            .ok_or_else(unknown);
    }
    date_parts(inner).map(Token::Date).ok_or_else(unknown)
}

fn number(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    text.parse().ok()
}

fn range(text: &str) -> Option<Range> {
    if text.is_empty() {
        return Some(Range::All);
    }
    if let Some((start, length)) = text.split_once(',') {
        return Some(Range::Length(number(start)?, length.parse().ok()?));
    }
    if let Some(start) = text.strip_suffix('-')
        && let Some(start) = number(start)
    {
        return Some(Range::From(start));
    }
    // The separating `-` is the first one after the start's own sign.
    let search_from = usize::from(text.starts_with('-'));
    match text[search_from..].find('-') {
        Some(dash) => {
            let dash = dash + search_from;
            Some(Range::Span(number(&text[..dash])?, number(&text[dash + 1..])?))
        }
        None => Some(Range::One(number(text)?)),
    }
}

fn counter(text: &str) -> Option<CounterParts> {
    let (body, digits) = match text.split_once(':') {
        Some((body, digits)) => (body, Some(digits.parse().ok()?)),
        None => (text, None),
    };
    let sign_after_first = body
        .char_indices()
        .skip(1)
        .find(|&(_, c)| c == '+' || c == '-')
        .map(|(i, _)| i);
    // A leading `-` is the start's sign only when a step follows it.
    let step_at = match body.chars().next() {
        Some('+') => Some(0),
        Some('-') => sign_after_first.or(Some(0)),
        _ => sign_after_first,
    };
    let (start, step) = match step_at {
        Some(at) => (&body[..at], Some(number(body[at..].trim_start_matches('+'))?)),
        None => (body, None),
    };
    let start = if start.is_empty() { None } else { Some(number(start)?) };
    Some(CounterParts { start, step, digits })
}

/// The `(start, step, digits)` a whole `[C…]` token sets, as typed: for the
/// tests that hold the frontend's parser to this one.
#[cfg(test)]
pub(super) fn counter_token(token: &str) -> Option<(Option<i64>, Option<i64>, Option<u32>)> {
    let inner = token.strip_prefix("[C")?.strip_suffix(']')?;
    counter(inner).map(|p| (p.start, p.step, p.digits))
}

/// `mask` with every counter's missing start / step / digits filled from these,
/// as an older sheet's own counter fields did; a part at its default fills
/// nothing. A counter that gains nothing, and the rest of the mask, keep their
/// text. For loading presets saved before the counter lived in the mask.
pub fn fill_counter_defaults(mask: &str, start: i64, step: i64, digits: u32) -> String {
    let digits = digits.clamp(1, MAX_COUNTER_DIGITS);
    let mut out = String::with_capacity(mask.len());
    let mut rest = mask;
    // The same bracket rules as `Mask::parse`.
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some(tail) = after.strip_prefix('[') {
            out.push_str("[[");
            rest = tail;
            continue;
        }
        let Some(close) = after.find(']') else {
            rest = &rest[open..];
            break;
        };
        let inner = &after[..close];
        let parts = inner.strip_prefix('C').and_then(counter);
        match parts {
            Some(parts) => {
                let filled = CounterParts {
                    start: parts.start.or((start != 1).then_some(start)),
                    step: parts.step.or((step != 1).then_some(step)),
                    digits: parts.digits.or((digits != 1).then_some(digits)),
                };
                let text = if filled == parts {
                    inner.to_string()
                } else {
                    filled.text()
                };
                let _ = write!(out, "[{text}]");
            }
            None => {
                let _ = write!(out, "[{inner}]");
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// `[d]`, `[t]`, or date letters with separators between them (`[Y-M-D h.m.s]`).
/// It starts with a letter, so `[-3-]` stays a range; `:` and `/` aren't
/// separators (macOS shows `:` as `/`, and `/` splits paths).
fn date_parts(text: &str) -> Option<Vec<DatePart>> {
    match text {
        "d" => return Some(vec![DatePart::IsoDate]),
        "t" => return Some(vec![DatePart::DottedTime]),
        _ => {}
    }
    let parts = text
        .chars()
        .map(|c| match c {
            'Y' => Some(DatePart::Year),
            'y' => Some(DatePart::ShortYear),
            'M' => Some(DatePart::Month),
            'D' => Some(DatePart::Day),
            'h' => Some(DatePart::Hour),
            'm' => Some(DatePart::Minute),
            's' => Some(DatePart::Second),
            '-' | '.' | '_' | ' ' => Some(DatePart::Separator(c)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    match parts.first() {
        Some(DatePart::Separator(_)) | None => None,
        Some(_) => Some(parts),
    }
}

/// 1-based `position` (negative counts from the end) as a 0-based index.
fn index_of(position: i64, len: usize) -> i64 {
    if position > 0 {
        position - 1
    } else {
        len as i64 + position
    }
}

fn slice(source: &str, range: Range) -> String {
    if range == Range::All {
        return source.to_string();
    }
    match bounds(range, source.chars().count()) {
        Some((first, last)) => source.chars().skip(first).take(last - first + 1).collect(),
        None => String::new(),
    }
}

/// The characters `range` takes from a text `len` characters long: the first and
/// last index, from 0, both included; `None` when it takes nothing (past the end,
/// or an empty text).
fn bounds(range: Range, len: usize) -> Option<(usize, usize)> {
    let (first, last) = match range {
        Range::All => (0, len as i64 - 1),
        Range::One(at) => (index_of(at, len), index_of(at, len)),
        Range::From(at) => (index_of(at, len), len as i64 - 1),
        Range::Length(at, length) => {
            let first = index_of(at, len);
            // Saturating: `[N5,9223372036854775807]` runs to the end, never wraps.
            let length = i64::try_from(length).unwrap_or(i64::MAX);
            (first, first.saturating_add(length).saturating_sub(1))
        }
        // A negative start counts the end from the end too: `[N-8-5]` is the
        // 8th-last to the 5th-last.
        Range::Span(at, to) if at < 0 && to > 0 => (index_of(at, len), index_of(-to, len)),
        Range::Span(at, to) => (index_of(at, len), index_of(to, len)),
    };
    let first = first.max(0);
    let last = last.min(len as i64 - 1);
    if first > last {
        return None;
    }
    // Both within 0..len here, so neither conversion can fail.
    Some((usize::try_from(first).ok()?, usize::try_from(last).ok()?))
}

fn pad(value: i64, digits: u32) -> String {
    let width = digits.clamp(1, MAX_COUNTER_DIGITS) as usize;
    if value < 0 {
        format!(
            "-{:0width$}",
            value.unsigned_abs(),
            width = width.saturating_sub(1).max(1)
        )
    } else {
        format!("{value:0width$}")
    }
}

fn date(when: &NaiveDateTime, parts: &[DatePart]) -> String {
    let mut out = String::new();
    for part in parts {
        let _ = match part {
            DatePart::Year => write!(out, "{:04}", when.year()),
            DatePart::ShortYear => write!(out, "{:02}", when.year().rem_euclid(100)),
            DatePart::Month => write!(out, "{:02}", when.month()),
            DatePart::Day => write!(out, "{:02}", when.day()),
            DatePart::Hour => write!(out, "{:02}", when.hour()),
            DatePart::Minute => write!(out, "{:02}", when.minute()),
            DatePart::Second => write!(out, "{:02}", when.second()),
            DatePart::IsoDate => write!(out, "{:04}-{:02}-{:02}", when.year(), when.month(), when.day()),
            DatePart::DottedTime => write!(out, "{:02}.{:02}.{:02}", when.hour(), when.minute(), when.second()),
            DatePart::Separator(c) => {
                out.push(*c);
                Ok(())
            }
        };
    }
    out
}

/// Appends text in the case the mask last switched to.
struct CaseWriter {
    text: String,
    case: Case,
}

impl Default for CaseWriter {
    fn default() -> Self {
        Self {
            text: String::new(),
            case: Case::Original,
        }
    }
}

impl CaseWriter {
    fn push(&mut self, piece: &str) {
        match self.case {
            Case::Original => self.text.push_str(piece),
            Case::Upper => self.text.extend(piece.chars().flat_map(char::to_uppercase)),
            Case::Lower => self.text.extend(piece.chars().flat_map(char::to_lowercase)),
            Case::Words => {
                for c in piece.chars() {
                    let starts_word = self
                        .text
                        .chars()
                        .next_back()
                        .is_none_or(|before| !before.is_alphanumeric());
                    if starts_word {
                        self.text.extend(c.to_uppercase());
                    } else {
                        self.text.extend(c.to_lowercase());
                    }
                }
            }
        }
    }
}
