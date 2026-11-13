//! Where an item came from, read from what the OS recorded on it.
//!
//! Every byte here is attacker-written: `com.apple.quarantine` and
//! `kMDItemWhereFroms` travel inside a zip's `__MACOSX` entries and on a USB disk,
//! so anyone can write "downloaded from apple.com". The decoders cap their input
//! first, a malformed field is absent (❌ never a guess), and every string comes
//! out as [`ItemText`]. The card words the result as what macOS recorded.
//!
//! [`redact_for_egress`] decides what of a source may leave the machine: a web
//! source's host, and for everything else its kind with no text.

use std::io::Cursor;

use serde::Serialize;
use url::{Host, Url};

use crate::fact::{Fact, UnknownWhy};
use crate::text::ItemText;

/// The largest provenance xattr the decoders read. A bigger one is unparseable.
pub const MAX_XATTR_BYTES: usize = 64 * 1024;

/// How many `kMDItemWhereFroms` entries are read; the rest are dropped.
pub(crate) const MAX_SOURCES: usize = 16;

/// The longest URL kept for the card's copy and open. A longer one keeps its
/// host and loses the link.
pub(crate) const MAX_URL_CHARS: usize = 2_048;

/// The longest DNS name. A longer "host" isn't one.
const MAX_HOST_CHARS: usize = 253;

/// The quarantine agent AirDrop's receiver writes.
const AIRDROP_AGENT: &str = "sharingd";

/// What the OS recorded about a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// Where it was downloaded from. Unknown with `NoRecord` when the OS recorded
    /// no source, with `Unparseable` when the record is there and unreadable.
    pub sources: Fact<Vec<WhereFrom>>,
    /// When, from the quarantine record. `None`: the record has no time.
    pub downloaded_at: Option<i64>,
    /// The app that downloaded it, from the quarantine record. `None`: the record
    /// names none.
    pub downloaded_by: Option<ItemText>,
}

/// The Cmdr operation that put the item where it is, from Cmdr's own log.
///
/// Claimed only for the very node the operation landed: a file replaced since
/// answers `NodeChangedSince`, never this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CmdrOperation {
    /// What the operation did.
    pub kind: LandedBy,
    /// When it finished.
    pub at: i64,
    /// Who started it.
    pub initiator: StartedBy,
    /// Where the item came from. `None`: the operation made it new.
    pub source_path: Option<ItemText>,
    /// The volume it came from, by name. `None`: the same volume, or the
    /// operation made it new.
    pub source_volume: Option<ItemText>,
}

/// How a Cmdr operation landed an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LandedBy {
    /// Copied here.
    Copy,
    /// Moved here.
    Move,
    /// Renamed to this name.
    Rename,
    /// Created here.
    Create,
    /// Extracted from an archive.
    Extract,
    /// Compressed into this archive.
    Compress,
}

/// Who started a Cmdr operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StartedBy {
    /// The person, in Cmdr's own UI.
    You,
    /// Ask Cmdr, on the person's behalf.
    AskCmdr,
    /// Another AI client driving Cmdr.
    AiClient,
}

/// One recorded source. Raw and local-only: [`redact_for_egress`] is what a model
/// gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WhereFrom {
    /// An `http` or `https` address.
    Web {
        /// The site, which is what the card shows.
        site: WebSite,
        /// The whole address, parsed and written back out, for the card's copy
        /// and open. `None`: it's longer than [`MAX_URL_CHARS`].
        url: Option<String>,
    },
    /// A Mail attachment: Mail records who sent it and the subject line.
    MailAttachment {
        /// The sender, as Mail wrote it. `None`: the record has none.
        sender: Option<ItemText>,
        /// The subject line. `None`: the record has none.
        subject: Option<ItemText>,
    },
    /// Received over AirDrop.
    AirDrop {
        /// The sending person's or device's name.
        sender: ItemText,
    },
    /// Anything else the record holds.
    Other {
        /// What kind of thing it is.
        kind: OtherSource,
        /// The entry's text.
        text: ItemText,
    },
}

/// The site of a web source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSite {
    /// A host name: lowercased, an international name in its punycode form
    /// (`xn--…`), so a look-alike can't pass for another site.
    Domain {
        /// The host name, with no port, user info, path, query, or fragment.
        host: String,
    },
    /// A numeric address, which names no site.
    IpAddress {
        /// The address.
        address: String,
    },
}

/// What a non-web source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OtherSource {
    /// A `data:` or `blob:` address: the address IS content.
    InlineData,
    /// A `file:` address: a local path.
    LocalFile,
    /// Text or an address Cmdr doesn't recognize.
    Unrecognized,
}

/// What of a source may leave the machine: a web source's host name, and for
/// everything else its kind with no text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EgressSource {
    /// A website, by host name alone.
    Site {
        /// The host name.
        host: String,
    },
    /// A numeric address.
    IpAddress,
    /// A Mail attachment.
    MailAttachment,
    /// An AirDrop transfer.
    AirDrop,
    /// A `data:` or `blob:` address.
    InlineData,
    /// A local file.
    LocalFile,
    /// Something else.
    Other,
}

/// A parsed `com.apple.quarantine` value: `flags;time;agent;event-id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Quarantine {
    /// The flag bits. `None`: the field isn't hex.
    pub(crate) flags: Option<u32>,
    /// When the item was quarantined, which for a download is when it was
    /// downloaded. `None`: the field is empty, zero, or not hex.
    pub(crate) at: Option<i64>,
    /// The app that quarantined it. `None`: the field is empty.
    pub(crate) agent: Option<ItemText>,
    /// The id of the event in the OS's quarantine database. `None`: the field
    /// isn't a UUID.
    pub(crate) event_id: Option<ItemText>,
}

/// Reads a `com.apple.quarantine` value. `None` when it's over the cap, not
/// text, or holds no readable field.
pub(crate) fn parse_quarantine(bytes: &[u8]) -> Option<Quarantine> {
    if bytes.len() > MAX_XATTR_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut fields = text.splitn(4, ';');
    let flags = fields
        .next()
        .and_then(|field| hex_field(field, 8))
        .map(|bits| bits as u32);
    let at = fields
        .next()
        .and_then(|field| hex_field(field, 15))
        .filter(|seconds| *seconds != 0)
        .map(|seconds| seconds as i64);
    let agent = fields
        .next()
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
        .map(ItemText::new);
    let event_id = fields.next().map(str::trim).filter(|id| is_uuid(id)).map(ItemText::new);
    let quarantine = Quarantine {
        flags,
        at,
        agent,
        event_id,
    };
    let empty = quarantine.flags.is_none()
        && quarantine.at.is_none()
        && quarantine.agent.is_none()
        && quarantine.event_id.is_none();
    (!empty).then_some(quarantine)
}

/// A hex field of at most `max_digits` digits, so the value fits its target.
fn hex_field(field: &str, max_digits: usize) -> Option<u64> {
    let field = field.trim();
    if field.is_empty() || field.len() > max_digits || !field.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(field, 16).ok()
}

fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The record is there and can't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unparseable;

/// Reads a `kMDItemWhereFroms` value: a binary property list holding an array of
/// strings. Anything else is [`Unparseable`]. At most [`MAX_SOURCES`] entries
/// come back, empty ones dropped.
pub(crate) fn decode_where_froms(bytes: &[u8]) -> Result<Vec<String>, Unparseable> {
    if bytes.len() > MAX_XATTR_BYTES || !bytes.starts_with(b"bplist00") {
        return Err(Unparseable);
    }
    let value = plist::Value::from_reader(Cursor::new(bytes)).map_err(|_| Unparseable)?;
    let entries = value.into_array().ok_or(Unparseable)?;
    let mut sources = Vec::new();
    for entry in entries {
        let text = entry.into_string().ok_or(Unparseable)?;
        if !text.is_empty() && sources.len() < MAX_SOURCES {
            sources.push(text);
        }
    }
    Ok(sources)
}

/// Types the entries of one download record. `downloaded_by` is the quarantine
/// agent, which is what tells an AirDrop sender's name from other text.
pub(crate) fn classify_sources(entries: &[String], downloaded_by: Option<&str>) -> Vec<WhereFrom> {
    let entries: Vec<&str> = entries
        .iter()
        .map(String::as_str)
        .filter(|entry| !entry.is_empty())
        .take(MAX_SOURCES)
        .collect();
    // Mail writes the sender, the subject, and a `message:` address. The first two are
    // free text, so they're read by position, never parsed.
    if entries.iter().any(|entry| has_scheme(entry, "message")) {
        let mut texts = entries.iter().filter(|entry| !has_scheme(entry, "message"));
        return vec![WhereFrom::MailAttachment {
            sender: texts.next().map(|text| ItemText::new(text)),
            subject: texts.next().map(|text| ItemText::new(text)),
        }];
    }
    let from_airdrop = downloaded_by.is_some_and(|agent| agent.eq_ignore_ascii_case(AIRDROP_AGENT));
    entries.into_iter().map(|entry| classify(entry, from_airdrop)).collect()
}

fn has_scheme(entry: &str, scheme: &str) -> bool {
    entry
        .trim_start()
        .split_once(':')
        .is_some_and(|(found, _)| found.eq_ignore_ascii_case(scheme))
}

/// Types one entry. The WHOLE entry is parsed, before any cut: a host read from a
/// truncated address could be a piece of its user info.
fn classify(entry: &str, from_airdrop: bool) -> WhereFrom {
    let other = |kind| WhereFrom::Other {
        kind,
        text: ItemText::new(entry),
    };
    let Ok(parsed) = Url::parse(entry) else {
        return if from_airdrop {
            WhereFrom::AirDrop {
                sender: ItemText::new(entry),
            }
        } else {
            other(OtherSource::Unrecognized)
        };
    };
    match parsed.scheme() {
        "http" | "https" => match web_site(&parsed) {
            Some(site) => WhereFrom::Web {
                site,
                url: Some(parsed.as_str())
                    .filter(|url| url.chars().count() <= MAX_URL_CHARS)
                    .map(str::to_owned),
            },
            None => other(OtherSource::Unrecognized),
        },
        "data" | "blob" => other(OtherSource::InlineData),
        "file" => other(OtherSource::LocalFile),
        _ => other(OtherSource::Unrecognized),
    }
}

/// The site of a parsed web address, or `None` when its host isn't a plain name
/// or a numeric address.
fn web_site(parsed: &Url) -> Option<WebSite> {
    match parsed.host()? {
        Host::Domain(domain) => {
            let host = domain.trim_end_matches('.');
            is_plain_host(host).then(|| WebSite::Domain { host: host.to_owned() })
        }
        Host::Ipv4(address) => Some(WebSite::IpAddress {
            address: address.to_string(),
        }),
        Host::Ipv6(address) => Some(WebSite::IpAddress {
            address: address.to_string(),
        }),
    }
}

/// Whether `host` is a lowercase ASCII DNS name: letters, digits, `-`, `_`, and
/// dots, at most 253 characters. The URL parser allows more (`!`, `+`, `'`), which
/// no real site uses and a hostile one would.
fn is_plain_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= MAX_HOST_CHARS
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
}

/// What of `source` may leave the machine.
///
/// A web address leaves as its host name alone: real ones carry Gmail attachment
/// tokens and signed CDN keys in the query, and document ids, private repo names,
/// and share tokens in the path, and no rule can tell a token from a folder name.
/// Everything else leaves as its kind: a `data:` address is the content itself, a
/// `file:` one is a local path, and Mail and AirDrop record people's names.
pub fn redact_for_egress(source: &WhereFrom) -> EgressSource {
    match source {
        WhereFrom::Web {
            site: WebSite::Domain { host },
            ..
        } if is_plain_host(host) => EgressSource::Site { host: host.clone() },
        WhereFrom::Web {
            site: WebSite::Domain { .. },
            ..
        } => EgressSource::Other,
        WhereFrom::Web {
            site: WebSite::IpAddress { .. },
            ..
        } => EgressSource::IpAddress,
        WhereFrom::MailAttachment { .. } => EgressSource::MailAttachment,
        WhereFrom::AirDrop { .. } => EgressSource::AirDrop,
        WhereFrom::Other { kind, .. } => match kind {
            OtherSource::InlineData => EgressSource::InlineData,
            OtherSource::LocalFile => EgressSource::LocalFile,
            OtherSource::Unrecognized => EgressSource::Other,
        },
    }
}

/// The download fact from macOS's two xattrs, each `None` when the item doesn't
/// carry it.
pub fn download_fact(where_froms: Option<&[u8]>, quarantine: Option<&[u8]>) -> Fact<Download> {
    let quarantine = quarantine.and_then(parse_quarantine);
    let downloaded_by = quarantine.as_ref().and_then(|record| record.agent.clone());
    let entries = where_froms.map(decode_where_froms);
    download_from(entries, quarantine.as_ref().and_then(|record| record.at), downloaded_by)
}

/// The download fact from the `user.xdg.origin.url` and `user.xdg.referrer.url`
/// xattrs a Linux browser writes: each one address as plain text.
pub fn xdg_download_fact(origin: Option<&[u8]>, referrer: Option<&[u8]>) -> Fact<Download> {
    let present: Vec<&[u8]> = [origin, referrer].into_iter().flatten().collect();
    let entries = (!present.is_empty()).then(|| {
        present
            .into_iter()
            .map(|bytes| {
                if bytes.len() > MAX_XATTR_BYTES {
                    return Err(Unparseable);
                }
                std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| Unparseable)
            })
            .collect::<Result<Vec<String>, Unparseable>>()
    });
    download_from(entries, None, None)
}

fn download_from(
    entries: Option<Result<Vec<String>, Unparseable>>,
    downloaded_at: Option<i64>,
    downloaded_by: Option<ItemText>,
) -> Fact<Download> {
    let sources = match entries {
        None => Fact::unknown(UnknownWhy::NoRecord),
        Some(Err(Unparseable)) => Fact::unknown(UnknownWhy::Unparseable),
        Some(Ok(entries)) => {
            let sources = classify_sources(&entries, downloaded_by.as_ref().map(ItemText::as_str));
            if sources.is_empty() {
                Fact::unknown(UnknownWhy::NoRecord)
            } else {
                Fact::Known(sources)
            }
        }
    };
    if downloaded_at.is_none()
        && downloaded_by.is_none()
        && let Some(why) = sources.unknown_why()
    {
        return Fact::unknown(why);
    }
    Fact::Known(Download {
        sources,
        downloaded_at,
        downloaded_by,
    })
}

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;
