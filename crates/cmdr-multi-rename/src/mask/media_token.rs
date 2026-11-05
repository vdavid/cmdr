//! The `[=source.field|modified:format]` placeholder: a file's EXIF or tag value
//! (`MediaField`), formatted by the field's kind. A child of `mask`, so it shares
//! the mask's own date letters, ranges, and padding.

use chrono::NaiveDateTime;
use cmdr_media_meta::{FieldKind, MediaFacts, MediaField, MediaValue};

use super::{
    DatePart, FieldGap, MAX_COUNTER_DIGITS, Range, RowFacts, RowMedia, Token, date, date_parts, pad, placeholder,
    range, slice,
};

/// The fallback a date field may name after its `|`.
const MODIFIED_FALLBACK: &str = "modified";

/// One `[=source.field…]` placeholder on its own. For the session's field
/// samples and picker, which show what a field gives on real files without
/// formatting anything themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaToken {
    field: MediaField,
    /// `|modified`: a file without the date takes its modified date.
    fallback: bool,
    format: MediaFormat,
}

/// A media field's `:format`, by the field's kind.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MediaFormat {
    /// None typed: a date reads as `[d] [t]`, the rest as is.
    Plain,
    Date(Vec<DatePart>),
    Digits(u32),
    Range(Range),
}

/// A media placeholder's text for one row, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldText {
    /// The file's own value.
    Value(String),
    /// The modified date, standing in for a missing one (`|modified`).
    Fallback(String),
}

impl FieldText {
    pub fn into_text(self) -> String {
        match self {
            Self::Value(text) | Self::Fallback(text) => text,
        }
    }
}

impl MediaToken {
    /// The media field `inner` (a placeholder's text between its brackets)
    /// spells, or `None` for any other placeholder or none.
    pub fn parse(inner: &str) -> Option<Self> {
        match placeholder(inner) {
            Ok(Token::Media(token)) => Some(token),
            _ => None,
        }
    }

    /// `field` in its default format, with no fallback: `[=exif.taken]`.
    pub fn of(field: MediaField) -> Self {
        Self {
            field,
            fallback: false,
            format: MediaFormat::Plain,
        }
    }

    /// The field it names; only the grammar tests ask.
    #[cfg(test)]
    pub fn field(&self) -> MediaField {
        self.field
    }
}

impl MediaToken {
    /// The text for `row`, or why it has none. A file still being read waits
    /// for its own value rather than taking the fallback: one may yet come.
    pub fn resolve(&self, row: &RowFacts<'_>) -> Result<FieldText, FieldGap> {
        let field = self.field;
        if let RowMedia::Read(facts) = row.media
            && let Some(text) = self.value_text(facts)
        {
            return Ok(FieldText::Value(text));
        }
        match row.media {
            RowMedia::Reading => Err(FieldGap::Reading),
            _ if self.fallback => row
                .modified
                .map(|when| FieldText::Fallback(self.date_text(&when)))
                .ok_or(FieldGap::NoValue { field }),
            RowMedia::Unreadable => Err(FieldGap::Unreadable { field }),
            RowMedia::Read(_) | RowMedia::NotMedia => Err(FieldGap::NoValue { field }),
        }
    }

    /// The field's text in `facts`, formatted; `None` when the file has none.
    pub fn value_text(&self, facts: &MediaFacts) -> Option<String> {
        Some(match facts.get(self.field)? {
            MediaValue::Date(when) => self.date_text(&when),
            MediaValue::Number(value) => match self.format {
                MediaFormat::Digits(digits) => pad(value, digits),
                _ => value.to_string(),
            },
            MediaValue::Text(text) => match self.format {
                MediaFormat::Range(range) => slice(&text, range),
                _ => text.into_owned(),
            },
        })
    }

    /// `when` in the token's date format, or as `[d] [t]` with none.
    fn date_text(&self, when: &NaiveDateTime) -> String {
        match &self.format {
            MediaFormat::Date(parts) => date(when, parts),
            _ => date(
                when,
                &[DatePart::IsoDate, DatePart::Separator(' '), DatePart::DottedTime],
            ),
        }
    }
}

/// A media field after its `=`: `exif.taken`, then `|modified` (dates only), then
/// `:format` by the field's kind. The suffixes come in that order; nothing may be
/// empty. A digit count runs 1 to `MAX_COUNTER_DIGITS`.
pub(super) fn media(text: &str) -> Option<MediaToken> {
    let (head, format) = match text.split_once(':') {
        Some((head, format)) => (head, Some(format)),
        None => (text, None),
    };
    let (name, fallback) = match head.split_once('|') {
        Some((name, fallback)) => (name, Some(fallback)),
        None => (head, None),
    };
    let field = MediaField::parse(name)?;
    let kind = field.kind();
    let fallback = match fallback {
        None => false,
        Some(MODIFIED_FALLBACK) if kind == FieldKind::Date => true,
        Some(_) => return None,
    };
    let format = match format {
        None => MediaFormat::Plain,
        Some("") => return None,
        Some(format) => match kind {
            FieldKind::Date => MediaFormat::Date(date_parts(format)?),
            FieldKind::Number => MediaFormat::Digits(
                format
                    .parse()
                    .ok()
                    .filter(|digits| (1..=MAX_COUNTER_DIGITS).contains(digits))?,
            ),
            FieldKind::Text => MediaFormat::Range(range(format)?),
        },
    };
    Some(MediaToken {
        field,
        fallback,
        format,
    })
}

/// What a whole media token (`[=tag.track:2]`) spells, as the shared vectors
/// write it: `{ field, fallback, format }`. For the tests that hold the
/// frontend's parser to this one.
#[cfg(test)]
pub(in crate::multi_rename) fn media_token_parts(token: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    let inner = token.strip_prefix('[')?.strip_suffix(']')?;
    let media = MediaToken::parse(inner)?;
    let format = match &media.format {
        MediaFormat::Plain => serde_json::Value::Null,
        MediaFormat::Date(parts) => {
            let text: String = parts
                .iter()
                .map(|part| match part {
                    DatePart::Year => 'Y',
                    DatePart::ShortYear => 'y',
                    DatePart::Month => 'M',
                    DatePart::Day => 'D',
                    DatePart::Hour => 'h',
                    DatePart::Minute => 'm',
                    DatePart::Second => 's',
                    DatePart::IsoDate => 'd',
                    DatePart::DottedTime => 't',
                    DatePart::Separator(c) => *c,
                })
                .collect();
            json!({ "kind": "date", "text": text })
        }
        MediaFormat::Digits(digits) => json!({ "kind": "digits", "digits": digits }),
        MediaFormat::Range(range) => {
            let range = match *range {
                Range::All => json!({ "kind": "all" }),
                Range::One(at) => json!({ "kind": "one", "at": at }),
                Range::From(at) => json!({ "kind": "from", "at": at }),
                Range::Length(at, length) => json!({ "kind": "length", "at": at, "length": length }),
                Range::Span(at, to) => json!({ "kind": "span", "at": at, "to": to }),
            };
            json!({ "kind": "range", "range": range })
        }
    };
    Some(json!({ "field": media.field.name(), "fallback": media.fallback, "format": format }))
}
