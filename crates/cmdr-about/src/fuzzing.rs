//! The property seam for `fuzz/fuzz_targets/about_provenance.rs`: the download
//! record's decoders against bytes a download wrote.
//!
//! It adds no logic. It calls the decoders the way the host does and asserts what
//! must hold for any input: text that reaches a row is plain and bounded, and
//! what leaves the machine is a host name or nothing.

use crate::fact::Fact;
use crate::provenance::{
    Download, EgressSource, MAX_SOURCES, MAX_URL_CHARS, WebSite, WhereFrom, decode_where_froms, download_fact,
    parse_quarantine, redact_for_egress, xdg_download_fact,
};
use crate::text::{ItemText, MAX_ITEM_TEXT_CHARS};

/// Runs every provenance decoder over `data`, then over a well-formed record
/// written from it.
pub fn provenance(data: &[u8]) {
    if let Some(quarantine) = parse_quarantine(data) {
        for text in [&quarantine.agent, &quarantine.event_id].into_iter().flatten() {
            check_text(text);
        }
    }
    check_download(&download_fact(Some(data), Some(data)));
    check_download(&xdg_download_fact(Some(data), None));

    // A record macOS could have written: the input's lines as the array's strings.
    let lines: Vec<String> = String::from_utf8_lossy(data)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    let mut record = Vec::new();
    plist::Value::Array(lines.iter().cloned().map(plist::Value::String).collect())
        .to_writer_binary(&mut record)
        .expect("a plist value writes to memory");
    if let Ok(entries) = decode_where_froms(&record) {
        assert!(entries.len() <= MAX_SOURCES, "more sources than the cap");
        assert!(
            entries.iter().eq(lines.iter().take(entries.len())),
            "a source read back as another string"
        );
    }
    check_download(&download_fact(Some(&record), Some(data)));
}

fn check_download(fact: &Fact<Download>) {
    let Some(download) = fact.known() else {
        return;
    };
    if let Some(app) = &download.downloaded_by {
        check_text(app);
    }
    for source in download.sources.known().into_iter().flatten() {
        check_source(source);
    }
}

fn check_source(source: &WhereFrom) {
    match source {
        WhereFrom::Web { site, url } => {
            match site {
                WebSite::Domain { host } => check_host(host),
                WebSite::IpAddress { address } => check_host(&address.replace(':', ".")),
            }
            if let Some(url) = url {
                assert!(url.chars().count() <= MAX_URL_CHARS, "a kept address is over its cap");
            }
        }
        WhereFrom::MailAttachment { sender, subject } => {
            for text in [sender, subject].into_iter().flatten() {
                check_text(text);
            }
        }
        WhereFrom::AirDrop { sender } => check_text(sender),
        WhereFrom::Other { text, .. } => check_text(text),
    }
    if let EgressSource::Site { host } = redact_for_egress(source) {
        check_host(&host);
    }
}

fn check_text(text: &ItemText) {
    assert!(
        !text.as_str().chars().any(reads_as_something_else),
        "an item's text carries a control or bidirectional character"
    );
    assert!(
        text.as_str().chars().count() <= MAX_ITEM_TEXT_CHARS + 1,
        "an item's text is over its cap"
    );
}

/// Stated here on its own, never through the sanitizer's predicate: a property
/// that asks the code under test what's unsafe can't catch it being wrong.
fn reads_as_something_else(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

/// A host is a DNS name and nothing else: no user info, port, path, query, or
/// fragment could ride in these characters.
fn check_host(host: &str) {
    assert!(
        !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')),
        "a host carries more than a host name"
    );
    assert!(host.len() <= 253, "a host is longer than a DNS name");
}

#[cfg(test)]
mod tests {
    /// The shapes the seed corpus holds (`fuzz/seeds/about_provenance/`), one per
    /// property, so a property that stops holding is red here before any fuzz run.
    #[test]
    fn the_properties_hold_for_the_shapes_the_seeds_carry() {
        let long_host = format!(
            "https://{}example/x",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.".repeat(5)
        );
        let twenty_sources: Vec<String> = (0..20).map(|n| format!("https://example.com/{n}")).collect();
        let inputs: [Vec<u8>; 9] = [
            b"0081;6af6442f;Chrome;0A1B2C3D-1111-4222-8333-444455556666".to_vec(),
            "0081;6af6442f;Chro\u{7}\u{202E}me;".as_bytes().to_vec(),
            format!("0081;6af6442f;{};", "A".repeat(1_000)).into_bytes(),
            long_host.into_bytes(),
            b"https://say+this.example/x".to_vec(),
            format!("https://cdn.example.com/{}", "p".repeat(5_000)).into_bytes(),
            twenty_sources.join("\n").into_bytes(),
            b" padded text \nhttps://example.com/x".to_vec(),
            b"https://user:hunter2@Example.COM:8443/private?key=abc#frag".to_vec(),
        ];
        for input in inputs {
            super::provenance(&input);
        }
    }
}
