//! The two request bodies S3 takes as XML.

use std::fmt::Write as _;

const XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

/// One finished part: its number (1-based) and the ETag `UploadPart` or
/// `UploadPartCopy` returned for it, quotes and all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedPart {
    pub number: u32,
    pub etag: String,
}

/// `CompleteMultipartUpload`, with explicit part numbers in ascending order.
pub(crate) fn complete_multipart_upload_body(parts: &[CompletedPart]) -> String {
    let mut body = format!("<CompleteMultipartUpload xmlns=\"{XMLNS}\">");
    for part in parts {
        let _ = write!(
            body,
            "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
            part.number,
            escape(&part.etag)
        );
    }
    body.push_str("</CompleteMultipartUpload>");
    body
}

/// `Delete` for `DeleteObjects` (at most 1,000 keys; the caller batches).
/// Quiet mode reports only failures.
pub(crate) fn delete_objects_body(keys: &[&str], quiet: bool) -> String {
    let mut body = format!("<Delete xmlns=\"{XMLNS}\"><Quiet>{quiet}</Quiet>");
    for key in keys {
        let _ = write!(body, "<Object><Key>{}</Key></Object>", escape(key));
    }
    body.push_str("</Delete>");
    body
}

/// XML-escapes text, plus CR and LF as character references: a parser turns
/// a literal CR or CRLF into LF, so a key holding one would otherwise name a
/// different object (AWS's object-key naming guide asks for exactly this).
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\r' => out.push_str("&#13;"),
            '\n' => out.push_str("&#10;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
#[path = "build_test.rs"]
mod build_test;
