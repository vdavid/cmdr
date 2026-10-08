//! Entry points for the fuzz targets in `fuzz/` (repo root).
//!
//! An S3-compatible server writes every byte of a response body, and the
//! transport hands it, lossily decoded, to the parsers in `xml`. The fuzzer is
//! looking for panics, runaway allocations, and hangs.

use crate::xml::{
    parse_complete_multipart, parse_copy_result, parse_delete_result, parse_error_body, parse_initiate_multipart,
    parse_list_buckets, parse_list_multipart_uploads, parse_list_objects,
};

/// Parses `body` as every response document this backend reads.
pub fn response_body(body: &[u8]) {
    let body = String::from_utf8_lossy(body);
    let _ = parse_list_buckets(&body);
    let _ = parse_list_objects(&body);
    let _ = parse_initiate_multipart(&body);
    let _ = parse_complete_multipart(&body);
    let _ = parse_copy_result(&body);
    let _ = parse_list_multipart_uploads(&body);
    let _ = parse_delete_result(&body);
    let _ = parse_error_body(&body);
}
