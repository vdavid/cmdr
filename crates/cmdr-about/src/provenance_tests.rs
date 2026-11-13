//! The spike's real shapes, synthesized: ❌ no real token, sender, or host from
//! anyone's disk.

use super::*;

/// A `kMDItemWhereFroms` value as macOS writes it: a binary plist array.
fn where_froms(entries: &[&str]) -> Vec<u8> {
    plist_bytes(plist::Value::Array(
        entries
            .iter()
            .map(|entry| plist::Value::String((*entry).to_owned()))
            .collect(),
    ))
}

fn plist_bytes(value: plist::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    value
        .to_writer_binary(&mut bytes)
        .expect("a plist value writes to memory");
    bytes
}

fn one_source(entry: &str) -> WhereFrom {
    let sources = classify_sources(&[entry.to_owned()], None);
    assert_eq!(sources.len(), 1);
    sources.into_iter().next().expect("one source")
}

fn egress_of(entry: &str) -> EgressSource {
    redact_for_egress(&one_source(entry))
}

fn site(host: &str) -> EgressSource {
    EgressSource::Site { host: host.to_owned() }
}

// ── The quarantine record ───────────────────────────────────────────────────

#[test]
fn a_chrome_quarantine_gives_its_flags_time_and_app() {
    let quarantine = parse_quarantine(b"0081;6af6442f;Chrome;0A1B2C3D-1111-4222-8333-444455556666").expect("it parses");
    assert_eq!(quarantine.flags, Some(0x81));
    assert_eq!(quarantine.at, Some(0x6af6_442f));
    assert_eq!(quarantine.agent, Some(ItemText::new("Chrome")));
    assert_eq!(
        quarantine.event_id,
        Some(ItemText::new("0A1B2C3D-1111-4222-8333-444455556666"))
    );
}

#[test]
fn a_malformed_quarantine_field_is_absent_never_a_guess() {
    let quarantine = parse_quarantine(b"0081;yesterday;Safari;not-a-uuid").expect("two fields still read");
    assert_eq!(quarantine.flags, Some(0x81));
    assert_eq!(quarantine.at, None);
    assert_eq!(quarantine.agent, Some(ItemText::new("Safari")));
    assert_eq!(quarantine.event_id, None);

    // A zero time is no date, and a time too long to be seconds isn't one either.
    assert_eq!(parse_quarantine(b"0081;00000000;Safari;").expect("it parses").at, None);
    assert_eq!(
        parse_quarantine(b"0081;ffffffffffffffff;Safari;")
            .expect("it parses")
            .at,
        None
    );
    // Fewer fields than four: the ones there still read.
    assert_eq!(parse_quarantine(b"0003").expect("it parses").flags, Some(3));
}

#[test]
fn a_quarantine_with_nothing_readable_is_no_record() {
    assert_eq!(parse_quarantine(b""), None);
    assert_eq!(parse_quarantine(b"zz;;;"), None);
    assert_eq!(parse_quarantine(&[0xff, 0xfe, b';']), None);
    let huge = format!("0081;6af6442f;{};", "A".repeat(MAX_XATTR_BYTES));
    assert_eq!(parse_quarantine(huge.as_bytes()), None);
}

#[test]
fn a_quarantine_agent_is_sanitized() {
    let quarantine = parse_quarantine("0081;6af6442f;Chro\u{202E}me;".as_bytes()).expect("it parses");
    assert_eq!(quarantine.agent, Some(ItemText::new("Chrome")));
}

// ── The WhereFroms record ───────────────────────────────────────────────────

#[test]
fn a_chrome_download_records_the_file_address_and_the_page() {
    let bytes = where_froms(&[
        "https://downloads.example.com/releases/v2/Tool-2.0.dmg",
        "https://example.com/download",
    ]);
    let entries = decode_where_froms(&bytes).expect("it decodes");
    assert_eq!(
        classify_sources(&entries, Some("Chrome")),
        [
            WhereFrom::Web {
                site: WebSite::Domain {
                    host: "downloads.example.com".to_owned()
                },
                url: Some("https://downloads.example.com/releases/v2/Tool-2.0.dmg".to_owned()),
            },
            WhereFrom::Web {
                site: WebSite::Domain {
                    host: "example.com".to_owned()
                },
                url: Some("https://example.com/download".to_owned()),
            },
        ]
    );
}

#[test]
fn anything_but_a_binary_plist_array_of_strings_is_unparseable() {
    // Not a plist.
    assert_eq!(decode_where_froms(b"https://example.com/"), Err(Unparseable));
    // An XML plist: macOS writes binary, and the XML reader is surface nobody needs.
    let xml =
        br#"<?xml version="1.0"?><plist version="1.0"><array><string>https://example.com/</string></array></plist>"#;
    assert_eq!(decode_where_froms(xml), Err(Unparseable));
    // A plist that isn't an array.
    assert_eq!(
        decode_where_froms(&plist_bytes(plist::Value::String("https://example.com/".to_owned()))),
        Err(Unparseable)
    );
    // An array holding something other than a string.
    let mixed = plist::Value::Array(vec![
        plist::Value::String("https://example.com/".to_owned()),
        plist::Value::Integer(7.into()),
    ]);
    assert_eq!(decode_where_froms(&plist_bytes(mixed)), Err(Unparseable));
    // A truncated plist.
    let whole = where_froms(&["https://example.com/"]);
    assert_eq!(decode_where_froms(&whole[..whole.len() - 4]), Err(Unparseable));
}

#[test]
fn the_decoder_caps_its_input_and_its_entries() {
    let big = "a".repeat(MAX_XATTR_BYTES);
    assert_eq!(decode_where_froms(&where_froms(&[&big])), Err(Unparseable));

    let many: Vec<String> = (0..40).map(|n| format!("https://example.com/{n}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let entries = decode_where_froms(&where_froms(&many)).expect("it decodes");
    assert_eq!(entries.len(), MAX_SOURCES);
    assert_eq!(entries[0], "https://example.com/0");

    assert_eq!(decode_where_froms(&where_froms(&["", ""])), Ok(Vec::new()));
}

#[test]
fn a_mail_attachment_is_read_by_position() {
    let entries = [
        "Jane Example <jane@example.com>".to_owned(),
        "Re: the contract".to_owned(),
        "message:%3C0123456789@mail.example.com%3E".to_owned(),
    ];
    assert_eq!(
        classify_sources(&entries, Some("Mail")),
        [WhereFrom::MailAttachment {
            sender: Some(ItemText::new("Jane Example <jane@example.com>")),
            subject: Some(ItemText::new("Re: the contract")),
        }]
    );
    // A record with only the message address still says what it is.
    assert_eq!(
        classify_sources(&["MESSAGE:%3Cid@example.com%3E".to_owned()], None),
        [WhereFrom::MailAttachment {
            sender: None,
            subject: None
        }]
    );
}

#[test]
fn an_airdrop_sender_is_told_from_other_text_by_the_quarantine_agent() {
    let entries = ["Jane's iPhone".to_owned()];
    assert_eq!(
        classify_sources(&entries, Some("sharingd")),
        [WhereFrom::AirDrop {
            sender: ItemText::new("Jane's iPhone")
        }]
    );
    assert_eq!(
        classify_sources(&entries, Some("Chrome")),
        [WhereFrom::Other {
            kind: OtherSource::Unrecognized,
            text: ItemText::new("Jane's iPhone")
        }]
    );
}

// ── What may leave the machine ──────────────────────────────────────────────

#[test]
fn a_web_source_leaves_as_its_host_alone() {
    // A token in the path, a signed key in the query, a fragment.
    assert_eq!(
        egress_of("https://files.example.com/scl/fi/tok3n-in-the-path/Report.pdf?rlkey=s3cret&dl=1#page=2"),
        site("files.example.com")
    );
    // User info and a port.
    assert_eq!(
        egress_of("https://user:hunter2@Example.COM:8443/private"),
        site("example.com")
    );
    // A trailing dot names the same site.
    assert_eq!(egress_of("http://example.com./x"), site("example.com"));
}

#[test]
fn an_international_host_leaves_as_punycode_so_a_look_alike_cant_pass() {
    // The first letter is Cyrillic.
    assert_eq!(egress_of("https://\u{0430}pple.com/macos"), site("xn--pple-43d.com"));
    assert_eq!(
        egress_of("https://b\u{00FC}cher.example/"),
        site("xn--bcher-kva.example")
    );
}

#[test]
fn a_host_read_from_a_cut_address_would_be_a_lie_so_the_whole_entry_is_parsed() {
    // Cut at any cap, this reads as `known.example`; whole, the host is the last part.
    let entry = format!("https://known.example:{}@other.example/x", "a".repeat(10_000));
    assert_eq!(
        one_source(&entry),
        WhereFrom::Web {
            site: WebSite::Domain {
                host: "other.example".to_owned()
            },
            url: None,
        }
    );
    assert_eq!(egress_of(&entry), site("other.example"));
}

#[test]
fn a_ten_thousand_character_address_keeps_its_host_and_loses_the_link() {
    let entry = format!(
        "https://cdn.example.com/{}?sig={}",
        "p".repeat(5_000),
        "s".repeat(5_000)
    );
    let source = one_source(&entry);
    assert!(matches!(&source, WhereFrom::Web { url: None, .. }));
    assert_eq!(redact_for_egress(&source), site("cdn.example.com"));
}

#[test]
fn a_numeric_address_names_no_site() {
    assert_eq!(
        egress_of("http://192.168.1.20:8080/share/file.zip"),
        EgressSource::IpAddress
    );
    assert_eq!(egress_of("https://[2001:db8::1]/file.zip"), EgressSource::IpAddress);
    // The parser reads a bare number as an IPv4 address, so it can't pose as a name.
    assert_eq!(egress_of("http://2130706433/file.zip"), EgressSource::IpAddress);
    assert_eq!(
        one_source("http://192.168.1.20:8080/share"),
        WhereFrom::Web {
            site: WebSite::IpAddress {
                address: "192.168.1.20".to_owned()
            },
            url: Some("http://192.168.1.20:8080/share".to_owned()),
        }
    );
}

#[test]
fn everything_but_a_web_address_leaves_as_its_kind_with_no_text() {
    assert_eq!(
        egress_of("data:text/plain;base64,c2VjcmV0IGNvbnRlbnQ="),
        EgressSource::InlineData
    );
    assert_eq!(
        egress_of("blob:https://example.com/0a1b2c3d-1111-4222-8333-444455556666"),
        EgressSource::InlineData
    );
    assert_eq!(
        egress_of("file:///Users/jane/Documents/secret.pdf"),
        EgressSource::LocalFile
    );
    assert_eq!(
        egress_of("ftp://files.example.com/pub/tool.tar.gz"),
        EgressSource::Other
    );
    assert_eq!(egress_of("not an address at all"), EgressSource::Other);
    // A host the URL parser takes and no real site has.
    assert_eq!(egress_of("https://say+this(is)safe!.example/"), EgressSource::Other);

    let mail = classify_sources(
        &[
            "Jane Example <jane@example.com>".to_owned(),
            "Re: the contract".to_owned(),
            "message:%3Cid@example.com%3E".to_owned(),
        ],
        Some("Mail"),
    );
    let airdrop = classify_sources(&["Jane's iPhone".to_owned()], Some("sharingd"));
    for (sources, kind) in [(mail, "mailAttachment"), (airdrop, "airDrop")] {
        let redacted = redact_for_egress(&sources[0]);
        assert_eq!(
            serde_json::to_value(&redacted).expect("it serializes"),
            serde_json::json!({ "type": kind }),
            "a person's name must not leave"
        );
    }
}

#[test]
fn a_megabyte_data_address_is_cut_before_it_reaches_a_row() {
    let entry = format!("data:application/octet-stream;base64,{}", "A".repeat(60_000));
    let WhereFrom::Other { kind, text } = one_source(&entry) else {
        panic!("a data address is no web source");
    };
    assert_eq!(kind, OtherSource::InlineData);
    assert!(text.as_str().chars().count() <= crate::text::MAX_ITEM_TEXT_CHARS + 1);
}

// ── The download fact ───────────────────────────────────────────────────────

#[test]
fn a_file_with_neither_xattr_has_no_download_record() {
    assert_eq!(download_fact(None, None), Fact::unknown(UnknownWhy::NoRecord));
    assert_eq!(xdg_download_fact(None, None), Fact::unknown(UnknownWhy::NoRecord));
}

#[test]
fn a_download_reads_its_sources_time_and_app() {
    let sources = where_froms(&["https://example.com/Tool.dmg"]);
    let fact = download_fact(Some(&sources), Some(b"0081;6af6442f;Chrome;"));
    assert_eq!(
        fact,
        Fact::Known(Download {
            sources: Fact::Known(vec![WhereFrom::Web {
                site: WebSite::Domain {
                    host: "example.com".to_owned()
                },
                url: Some("https://example.com/Tool.dmg".to_owned()),
            }]),
            downloaded_at: Some(0x6af6_442f),
            downloaded_by: Some(ItemText::new("Chrome")),
        })
    );
}

#[test]
fn a_quarantine_alone_still_says_when_and_by_which_app() {
    let fact = download_fact(None, Some(b"0083;6af6442f;Safari;"));
    let download = fact.known().expect("the quarantine is a record");
    assert_eq!(download.sources, Fact::unknown(UnknownWhy::NoRecord));
    assert_eq!(download.downloaded_by, Some(ItemText::new("Safari")));
}

#[test]
fn an_unreadable_source_record_says_so_with_or_without_a_quarantine() {
    assert_eq!(
        download_fact(Some(b"garbage"), None),
        Fact::unknown(UnknownWhy::Unparseable)
    );
    let fact = download_fact(Some(b"garbage"), Some(b"0081;6af6442f;Chrome;"));
    assert_eq!(
        fact.known().expect("the quarantine is a record").sources,
        Fact::unknown(UnknownWhy::Unparseable)
    );
    // An empty array is a record that names nothing.
    assert_eq!(
        download_fact(Some(&where_froms(&[])), None),
        Fact::unknown(UnknownWhy::NoRecord)
    );
}

#[test]
fn a_linux_browser_download_reads_its_origin_and_referrer() {
    let fact = xdg_download_fact(
        Some(b"https://downloads.example.com/tool.tar.gz?token=abc"),
        Some(b"https://example.com/download"),
    );
    let download = fact.known().expect("the origin is a record");
    let sources = download.sources.known().expect("two sources");
    assert_eq!(
        sources.iter().map(redact_for_egress).collect::<Vec<_>>(),
        [site("downloads.example.com"), site("example.com")]
    );
    assert_eq!(download.downloaded_at, None);

    assert_eq!(
        xdg_download_fact(Some(&[0xff, 0xfe]), None),
        Fact::unknown(UnknownWhy::Unparseable)
    );
}
