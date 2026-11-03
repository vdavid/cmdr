//! Entry points for the fuzz targets in `fuzz/` (repo root).
//!
//! An S3-compatible server writes every byte of a response body, and the
//! transport hands it, lossily decoded, to the parsers in `xml`. The fuzzer is
//! looking for panics, runaway allocations, and hangs, plus two round trips: a
//! key we send in a `DeleteObjects` body, echoed back the way S3 echoes it, and
//! a listing written the way S3 writes one, both read back as exactly what was
//! written. Every assertion has a failing mutant on record in
//! `fuzz/fuzz_targets/s3_xml.mutants.json`.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::xml::build::delete_objects_body;
use crate::xml::{
    parse_complete_multipart, parse_copy_result, parse_delete_result, parse_error_body, parse_initiate_multipart,
    parse_list_buckets, parse_list_multipart_uploads, parse_list_objects,
};

/// What S3 percent-encodes in a listing's keys under `encoding-type=url`:
/// everything but the unreserved characters and `/`. A space goes out as `+`.
const LISTED_KEY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// Parses `body` as every response document this backend reads, then reads
/// back the round trips written from the same bytes.
pub fn response_body(body: &[u8]) {
    let text = String::from_utf8_lossy(body);
    let _ = parse_list_buckets(&text);
    let _ = parse_list_objects(&text);
    let _ = parse_initiate_multipart(&text);
    let _ = parse_complete_multipart(&text);
    let _ = parse_copy_result(&text);
    let _ = parse_list_multipart_uploads(&text);
    let _ = parse_delete_result(&text);
    let _ = parse_error_body(&text);

    let (size, key) = split_size_and_key(body);
    delete_round_trip(&key);
    listing_round_trip(size, &key);
}

/// `[size: 8][key…]`, the size zero-padded and the key lossily decoded.
fn split_size_and_key(data: &[u8]) -> (u64, String) {
    let mut head = [0u8; 8];
    let taken = data.len().min(head.len());
    head[..taken].copy_from_slice(&data[..taken]);
    let key = String::from_utf8_lossy(data.get(8..).unwrap_or_default()).into_owned();
    (u64::from_le_bytes(head), key)
}

/// A `DeleteObjects` request names `key`; S3 answers with each deleted key in a
/// `<Deleted>` element, escaped the same way. That answer must name `key` again,
/// or a delete we confirmed would be for some other object.
fn delete_round_trip(key: &str) {
    if !xml_can_carry(key) {
        return;
    }
    let echoed = delete_objects_body(&[key], false)
        .replacen("<Delete ", "<DeleteResult ", 1)
        .replace("</Delete>", "</DeleteResult>")
        .replace("<Object>", "<Deleted>")
        .replace("</Object>", "</Deleted>");
    let outcome = parse_delete_result(&echoed).expect("an echoed delete parses");
    assert_eq!(outcome.deleted, [key], "a deleted key read back as another key");
}

/// A one-object listing, written literally or URL-encoded (as we ask for), must
/// read back with the key, its folder-style prefix, and the size as written.
fn listing_round_trip(size: u64, key: &str) {
    let encoded = |text: &str| utf8_percent_encode(text, LISTED_KEY).to_string().replace("%20", "+");
    let prefix = format!("{key}/");
    let mut forms = vec![("<EncodingType>url</EncodingType>", encoded(key), encoded(&prefix))];
    if xml_can_carry(key) {
        forms.push(("", escape(key), escape(&prefix)));
    }
    for (encoding, listed_key, listed_prefix) in forms {
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Name>bucket</Name>{encoding}<KeyCount>2</KeyCount><IsTruncated>false</IsTruncated>\
             <Contents><Key>{listed_key}</Key><Size>{size}</Size><StorageClass>STANDARD</StorageClass></Contents>\
             <CommonPrefixes><Prefix>{listed_prefix}</Prefix></CommonPrefixes>\
             </ListBucketResult>"
        );
        let page = parse_list_objects(&body).expect("a well-formed listing parses");
        assert_eq!(page.objects.len(), 1, "one object read back as {}", page.objects.len());
        assert_eq!(page.objects[0].key, key, "a listed key read back as another key");
        assert_eq!(page.objects[0].size, size, "a listed size read back wrong");
        assert_eq!(
            page.prefixes,
            [prefix.as_str()],
            "a listed prefix read back as another prefix"
        );
    }
}

/// XML-escapes `text` the way S3 writes a literal key: the five predefined
/// entities, plus CR as a character reference so a parser can't fold it.
fn escape(text: &str) -> String {
    quick_xml::escape::escape(text).replace('\r', "&#13;")
}

/// Whether XML 1.0 can carry `text` as character data at all (its `Char`
/// production). A key outside it is why we ask for `encoding-type=url`.
fn xml_can_carry(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..))
}
