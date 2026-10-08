//! One entity, one token: Bonjour instance names and `.local` hostnames in the host domain,
//! and a path's leaf tokenized whole whichever way a line prints it.

use super::*;

const TEST_PROCESS_SECRET: [u8; 32] = [0x6e; 32];

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-CONSIST")
}

/// Every `<kind:hash>` token in `line`, hash only.
fn hashes(line: &str) -> Vec<String> {
    Regex::new(r"<[a-z0-9-]+:([0-9a-f]{12})>")
        .expect("valid token regex")
        .captures_iter(line)
        .map(|caps| caps[1].to_string())
        .collect()
}

#[test]
fn a_bonjour_instance_name_is_a_host_token_and_the_service_type_stays() {
    let context = context();
    let prose = context
        .redact_line("call queriers to resolve Naspolya._smb._tcp.local. now")
        .into_owned();
    let keyed = context
        .redact_line(r#"No Keychain credentials: server="Naspolya._smb._tcp.local", share="Multimedia""#)
        .into_owned();
    let bare_host = context.redact_line(r#"Resolved: server="naspolya""#).into_owned();

    for line in [&prose, &keyed, &bare_host] {
        assert!(!line.to_lowercase().contains("naspolya"), "{line}");
        assert!(!line.contains("Multimedia"), "{line}");
    }
    assert!(
        prose.contains("<host:") && prose.contains(">._smb._tcp.local. now"),
        "{prose}"
    );
    assert!(keyed.contains(">._smb._tcp.local\""), "{keyed}");
    let token = hashes(&bare_host)[0].clone();
    assert_eq!(
        hashes(&prose)[0],
        token,
        "the instance name correlates with the host: {prose}"
    );
    assert_eq!(hashes(&keyed)[0], token, "{keyed}");
}

#[test]
fn a_bare_service_type_and_multi_label_local_hosts_redact_whole() {
    let context = context();
    assert_eq!(
        context.redact_line("mDNS SearchStarted: _smb._tcp.local."),
        "mDNS SearchStarted: _smb._tcp.local."
    );
    let multi = context.redact_line("dialing nas.home-lab.local:445").into_owned();
    assert!(!multi.contains("nas") && !multi.contains("home-lab"), "{multi}");
    assert!(multi.contains(".local:445"), "{multi}");
}

#[test]
fn a_leaf_with_a_space_gets_one_token_in_every_path_shape() {
    let context = context();
    let url = "sftp://ada@127.0.0.1:12480/srv/data/Anna Kovacs/Medical records";
    let lines = [
        format!("read_directory_with_progress: listing_id=l-1, path={url}"),
        format!("ended in error: PermissionDenied {{ path: {url:?} }} (NeedsAction)"),
        format!("loadDirectory called: path={url:?}, currentLoading=false"),
    ];
    let redacted: Vec<String> = lines
        .iter()
        .map(|line| context.redact_line(line).into_owned())
        .collect();
    for line in &redacted {
        for private in ["Anna", "Kovacs", "Medical", "records", "ada"] {
            assert!(!line.contains(private), "{private:?} survived: {line}");
        }
    }
    let leaf = |line: &str| hashes(line).last().cloned();
    assert_eq!(leaf(&redacted[0]), leaf(&redacted[1]), "{redacted:?}");
    assert_eq!(leaf(&redacted[0]), leaf(&redacted[2]), "{redacted:?}");

    // The typed SFTP path (no scheme) shares every segment token with the URL form.
    let typed = context
        .redact_line(r#"SFTP path="/srv/data/Anna Kovacs/Medical records": error_kind=permission_denied"#)
        .into_owned();
    let typed_hashes = hashes(&typed);
    let url_hashes = hashes(&redacted[2]);
    assert_eq!(
        typed_hashes,
        url_hashes[url_hashes.len() - typed_hashes.len()..].to_vec(),
        "{typed} vs {}",
        redacted[2]
    );
}

/// One address reads the same whether a keyed field or bare prose carries it.
#[test]
fn an_address_gets_one_label_in_every_field() {
    let context = context();
    let redacted = context
        .redact_line(r#"list: host="127.0.0.1", ip_address=Some("127.0.0.1"), addr=127.0.0.1:445, peer fe80::1"#)
        .into_owned();
    let labels: Vec<&str> = redacted.matches("<ipv4-loopback:").collect();
    assert_eq!(labels.len(), 3, "{redacted}");
    assert!(!redacted.contains("<ipv4:"), "{redacted}");
    assert!(redacted.contains("<ipv6-link-local:"), "{redacted}");
    let tokens = hashes(&redacted);
    assert_eq!(tokens[0], tokens[1], "{redacted}");
    assert_eq!(tokens[0], tokens[2], "{redacted}");
}

/// A dot in a name isn't an extension: `Anna.Kovacs` must not keep `.Kovacs`. Short lowercase
/// extensions, camera-style uppercase ones, and a few known long ones stay.
#[test]
fn only_conservative_extensions_survive() {
    let context = context();
    for (name, kept) in [
        ("Anna.Kovacs", None),
        ("minutes.2026", None),
        ("report.docx", Some(".docx")),
        ("IMG_0001.JPG", Some(".JPG")),
        ("library.sqlite3", Some(".sqlite3")),
        ("budget.numbers", Some(".numbers")),
        ("archive.tar.gz", Some(".gz")),
    ] {
        let redacted = context.redact_line(&format!(r#"x path="docs/{name}""#)).into_owned();
        match kept {
            Some(extension) => assert!(redacted.ends_with(&format!(">{extension}\"")), "{name} → {redacted}"),
            None => {
                let after = name.split('.').nth(1).unwrap_or_default();
                assert!(!redacted.contains(after), "{name} → {redacted}");
            }
        }
    }
}

/// A real folder can be named `<anna-kovacs>`; only a token with its hash (or a bare
/// placeholder word unsalted redaction writes) counts as already redacted.
#[test]
fn a_folder_named_like_a_token_without_a_hash_is_still_tokenized() {
    let context = context();
    let redacted = context
        .redact_line(r#"listing path="docs/<anna-kovacs>/<dir>/plan.pdf" failed"#)
        .into_owned();
    assert!(!redacted.contains("anna-kovacs"), "{redacted}");
    assert!(redacted.contains("/<dir>/"), "a bare placeholder stays: {redacted}");
    assert_eq!(context.redact_line(&redacted), redacted, "idempotent");
}
