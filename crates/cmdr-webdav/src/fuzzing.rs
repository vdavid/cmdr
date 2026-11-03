//! Entry points for the fuzz targets in `fuzz/` (repo root).
//!
//! A WebDAV server writes every byte of a PROPFIND answer, and a listing hands
//! it to [`parse_multistatus`] (the transport decodes it lossily first, as here).
//! The fuzzer is looking for panics, runaway allocations, and hangs, plus the
//! round trip below: an answer written the way a server writes one reads back as
//! exactly what it says. Every assertion has a failing mutant on record in
//! `fuzz/fuzz_targets/webdav_propfind.mutants.json`.

use std::time::{Duration, UNIX_EPOCH};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use quick_xml::escape::escape;
use url::Url;

use crate::propfind::{every_propstat_succeeded, parse_multistatus};
use crate::urls::join_remote;

/// The share every round-trip path sits under, as a Nextcloud user's root.
const BASE: &str = "https://dav.example/remote.php/dav/files/fuzz/";

/// The largest second an HTTP date or a four-digit RFC 3339 year can carry:
/// 9999-12-31T23:59:59Z.
const LAST_SECOND: u64 = 253_402_300_799;

/// What a server that encodes less than we do still has to encode in a path
/// segment: everything but RFC 3986's `pchar`s. `&`, `+`, `'`, `=`, and the
/// other sub-delimiters arrive raw, so the XML escaping is what carries them.
const LOOSE_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@');

/// Parses `body` as a PROPFIND `multistatus` answer and as a PROPPATCH one,
/// then reads back an answer written from the same bytes.
pub fn propfind(body: &[u8]) {
    let text = String::from_utf8_lossy(body);
    let _ = parse_multistatus(&text);
    let _ = every_propstat_succeeded(&text);
    round_trip(body);
}

/// Reads `data` as `[size: 8][modified: 8][created: 8][shape: 1][path…]`
/// (zero-padded), writes the PROPFIND answer a server would give for that file,
/// and checks every field reads back as written.
///
/// The `shape` bits pick how the server writes it: an absolute or a path-only
/// `href`, our percent-encoding or a looser one (so XML entities carry `&` and
/// friends), a collection or a file, and `creationdate`'s zone and fraction.
fn round_trip(data: &[u8]) {
    let mut head = [0u8; 25];
    let taken = data.len().min(head.len());
    head[..taken].copy_from_slice(&data[..taken]);
    let word = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().expect("eight bytes"));
    let size = word(0);
    let modified = word(8) % (LAST_SECOND + 1);
    // A day of margin each side, so a zone offset can't push the local time out
    // of the four-digit years; 1970-01-01 itself means "unknown" to the parser.
    let created = 86_400 + word(16) % (LAST_SECOND - 2 * 86_400);
    let shape = head[24];
    let absolute = shape & 1 != 0;
    let loose = shape & 2 != 0;
    let is_collection = shape & 4 != 0;
    let offset_minutes = match (shape >> 3) & 3 {
        0 => None,
        1 => Some(0),
        2 => Some(i64::from(shape >> 5) * 97),
        _ => Some(-i64::from(shape >> 5) * 97),
    };
    let fraction = shape & 0x80 != 0;

    // Names are what a listing holds: never empty, `.`, or `..`, none of which a
    // server can store.
    let path = String::from_utf8_lossy(data.get(25..).unwrap_or_default()).into_owned();
    let segments: Vec<&str> = path.split('/').filter(|s| !matches!(*s, "" | "." | "..")).collect();
    let base = Url::parse(BASE).expect("a fixed URL");
    let url = join_remote(&base, &segments.join("/"), is_collection);
    let mut href = if absolute {
        url.to_string()
    } else {
        url.path().to_string()
    };
    if loose {
        let decoded = percent_decode_str(&href).decode_utf8_lossy().into_owned();
        let (prefix, rest) = decoded.split_at(decoded.find("/remote.php").expect("under the base"));
        href = format!("{prefix}{}", reencode(rest));
    }

    let mut expected_href = "/remote.php/dav/files/fuzz".to_string();
    for segment in &segments {
        expected_href.push('/');
        expected_href.push_str(segment);
    }
    if is_collection {
        expected_href.push('/');
    }
    let resource_type = if is_collection { "<d:collection/>" } else { "" };
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <d:multistatus xmlns:d=\"DAV:\"><d:response>\
         <d:href>{href}</d:href>\
         <d:propstat><d:prop><d:getcontentlength>{other_size}</d:getcontentlength></d:prop>\
         <d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>\
         <d:propstat><d:prop>\
         <d:resourcetype>{resource_type}</d:resourcetype>\
         <d:getcontentlength>{size}</d:getcontentlength>\
         <d:getlastmodified>{modified_text}</d:getlastmodified>\
         <d:creationdate>{created_text}</d:creationdate>\
         </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>\
         </d:response></d:multistatus>",
        href = escape(href.as_str()),
        other_size = size.wrapping_add(1),
        modified_text = httpdate::fmt_http_date(UNIX_EPOCH + Duration::from_secs(modified)),
        created_text = rfc3339(created, offset_minutes, fraction),
    );

    let entries = parse_multistatus(&body).expect("a well-formed multistatus parses");
    assert_eq!(entries.len(), 1, "one response read back as {} entries", entries.len());
    let entry = &entries[0];
    assert_eq!(entry.href, expected_href, "the href read back as another path");
    assert_eq!(entry.is_collection, is_collection, "resourcetype read back wrong");
    assert_eq!(entry.size, Some(size), "the size read back wrong");
    assert_eq!(
        entry.modified_at,
        Some(modified),
        "getlastmodified read back as another time"
    );
    assert_eq!(
        entry.created_at,
        Some(created),
        "creationdate read back as another time"
    );
}

/// Re-encodes a decoded path the loose way, segment by segment.
fn reencode(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, LOOSE_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// `secs` as RFC 3339: `Z` without an offset, else the local time and its
/// `+HH:MM`/`-HH:MM`, optionally with milliseconds.
fn rfc3339(secs: u64, offset: Option<i64>, fraction: bool) -> String {
    let local = secs as i64 + offset.unwrap_or(0) * 60;
    let (days, of_day) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    let zone = match offset {
        None => "Z".to_string(),
        Some(m) => format!(
            "{}{:02}:{:02}",
            if m < 0 { '-' } else { '+' },
            m.abs() / 60,
            m.abs() % 60
        ),
    };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{}{zone}",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        if fraction { ".123" } else { "" },
    )
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Howard Hinnant's
/// `civil_from_days`, the inverse of the parser's `days_from_civil`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
