//! External-text fields (`detail=`, `stderr=`, `stdout=`): full in local logs, redacted and
//! capped in reports.

use super::*;

const TEST_PROCESS_SECRET: [u8; 32] = [0x3d; 32];

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-DETAIL")
}

/// The unescaped value of the first `key="…"` field in `line`.
fn field_value(line: &str, key: &str) -> String {
    let start = line.find(&format!("{key}=\"")).expect("field present") + key.len() + 2;
    let mut end = start;
    let bytes = line.as_bytes();
    while end < bytes.len() && bytes[end] != b'"' {
        end += if bytes[end] == b'\\' { 2 } else { 1 };
    }
    unescape_debug(&line[start..end]).into_owned()
}

#[test]
fn report_mode_redacts_identities_inside_detail_and_keeps_the_rest() {
    let line = format!(
        "smbclient share listing stopped: host=\"nas.local\", share=\"Client Plans\", code=Some(1), nt_status=STATUS_ACCESS_DENIED, detail={:?}",
        "tree connect failed for \"Client Plans\" at /Users/alice/Client Plans/budget.xlsx on 10.0.0.7 (alice@example.com)"
    );
    let redacted = context().redact_line(&line).into_owned();

    for private in [
        "alice",
        "Client Plans",
        "budget",
        "10.0.0.7",
        "nas.local",
        "example.com",
    ] {
        assert!(!redacted.contains(private), "{private:?} survived: {redacted}");
    }
    for kept in [
        "code=Some(1)",
        "nt_status=STATUS_ACCESS_DENIED",
        "tree connect failed for",
        "$HOME/",
        ".xlsx",
        "<email>",
    ] {
        assert!(redacted.contains(kept), "{kept:?} was lost: {redacted}");
    }
}

#[test]
fn report_mode_caps_detail_and_keeps_the_field_well_formed() {
    let long = format!("{}\nsecond line \\ \"quoted\" {}", "x".repeat(150), "y".repeat(400));
    for key in ["detail", "stderr", "stdout"] {
        let line = format!("tool stopped: {key}={long:?}, after=1");
        let redacted = context().redact_line(&line).into_owned();
        let value = field_value(&redacted, key);
        assert!(
            value.chars().count() <= REPORT_DETAIL_MAX_CHARS,
            "{key} not capped: {} chars",
            value.chars().count()
        );
        assert!(value.ends_with('…'), "no truncation marker: {value:?}");
        assert!(value.starts_with(&"x".repeat(150)), "the head must survive: {value:?}");
        assert!(
            redacted.ends_with(", after=1"),
            "text after the field must survive: {redacted}"
        );
    }
}

#[test]
fn report_mode_cap_never_leaves_a_dangling_escape() {
    // 199 plain chars then a backslash: a byte-level cut would leave `\"` and swallow the
    // closing quote.
    let value = format!("{}\\{}", "a".repeat(REPORT_DETAIL_MAX_CHARS - 2), "b".repeat(50));
    let line = format!("x detail={value:?} tail=1");
    let redacted = context().redact_line(&line).into_owned();
    assert!(redacted.ends_with("\" tail=1"), "closing quote lost: {redacted}");
    assert!(field_value(&redacted, "detail").chars().count() <= REPORT_DETAIL_MAX_CHARS);
}

#[test]
fn short_detail_passes_uncapped_and_redaction_is_idempotent() {
    let line = format!("x detail={:?}", "connection reset by peer");
    let context = context();
    let once = context.redact_line(&line).into_owned();
    assert_eq!(once, line);

    let line = format!("x detail={:?}", format!("{} /Users/alice/a.txt", "z".repeat(300)));
    let once = context.redact_line(&line).into_owned();
    assert_eq!(context.redact_line(&once), once);
}

#[test]
fn detail_tokens_correlate_with_the_rest_of_the_report() {
    let context = context();
    let path_line = context
        .redact_line(r#"SFTP path="/home/alice/Client Plans/report.pdf": error_kind=no_such_file"#)
        .into_owned();
    let detail_line = context
        .redact_line(&format!(
            "SFTP detail={:?}",
            "no such file: /home/alice/Client Plans/report.pdf"
        ))
        .into_owned();
    let token = Regex::new(r"<file:[0-9a-f]{12}>").expect("valid regex");
    let from_path = token.find(&path_line).expect("path token").as_str();
    assert!(
        detail_line.contains(from_path),
        "the same file must carry one token across fields: {path_line} vs {detail_line}"
    );
}

#[test]
fn unsalted_mode_redacts_and_caps_like_a_report() {
    let text = format!("/Users/alice/secret.txt {}", "q".repeat(300));
    let line = format!("x detail={text:?}");
    let redacted = redact_line(&line).into_owned();
    assert!(!redacted.contains("alice"), "{redacted}");
    assert!(redacted.contains("$HOME/<file>.txt"), "{redacted}");
    assert!(
        !redacted.contains(&"q".repeat(300)),
        "the cap applies to bare tokens too: {redacted}"
    );
    assert!(redacted.ends_with("…\""), "{redacted}");
}

#[test]
fn keyed_identities_are_scrubbed_where_the_prose_repeats_them_bare() {
    let context = context();
    // The detail comes first: the scrub reads the whole line, not just what precedes it.
    let line = format!(
        "SmbVolume::read detail={:?}, share=\"Plans\", user=\"ada\", path=\"docs/Quarterly Report.pdf\"",
        "tree connect to \"Plans\" as ada failed; Planscape unaffected; Quarterly Report.pdf is locked"
    );
    let redacted = context.redact_line(&line).into_owned();
    let detail = field_value(&redacted, "detail");
    for private in ["\"Plans\"", " ada ", "Quarterly Report"] {
        assert!(!detail.contains(private), "{private:?} survived in {detail:?}");
    }
    assert!(
        detail.contains("Planscape"),
        "a longer word must not be split: {detail:?}"
    );
    let share_token = Regex::new(r#"share="(<share:[0-9a-f]{12}>)""#)
        .expect("valid regex")
        .captures(&redacted)
        .expect("share token")[1]
        .to_string();
    assert!(
        detail.contains(&share_token),
        "prose must reuse the field's token: {redacted}"
    );
}

/// Pins the known gap: a name the line doesn't key anywhere has nothing to match it by.
#[test]
fn an_unkeyed_bare_name_in_prose_survives() {
    let line = format!("x detail={:?}", "tree connect failed: \"Unkeyed Share\"");
    let redacted = context().redact_line(&line).into_owned();
    assert!(redacted.contains("Unkeyed Share"), "{redacted}");
}

/// The frontend logs typed errors as `JSON.stringify(error)`, so identities arrive as
/// `"server":"…"` pairs, in a spelling the line's keyed fields may not share.
#[test]
fn report_mode_tokenizes_identity_pairs_in_json_inside_detail() {
    let context = context();
    let json = r#"{"type":"authFailed","message":"kept","server":"NASPOLYA","share":"Private Share","username":"anna","path":"/srv/Private Share/x.txt"}"#;
    let line = format!("Mount of share=\"Private Share\" on host=\"Naspolya\" did not go through: detail={json:?}");
    let redacted = context.redact_line(&line).into_owned();
    for private in ["NASPOLYA", "Naspolya", "Private Share", "anna"] {
        assert!(!redacted.contains(private), "{private:?} survived: {redacted}");
    }
    for kept in [
        r#"\"type\":\"authFailed\""#,
        r#"\"message\":\"kept\""#,
        r#"\"server\":\"<host:"#,
    ] {
        assert!(redacted.contains(kept), "{kept:?} lost: {redacted}");
    }
    let host_token = Regex::new(r#"host="(<host:[0-9a-f]{12}>)""#)
        .expect("valid regex")
        .captures(&redacted)
        .expect("host token")[1]
        .to_string();
    assert!(
        redacted.contains(&format!(r#"\"server\":\"{host_token}\""#)),
        "the JSON server correlates with the keyed host, case aside: {redacted}"
    );
}

/// A server or frontend error names paths under any prefix (`/srv`, `/data`, `/mnt`), which the
/// line scanner has no rule for. Inside external text, every absolute path is tokenized.
#[test]
fn report_mode_tokenizes_absolute_paths_with_any_prefix_inside_detail() {
    let context = context();
    let line = format!(
        "FE:uncaught Unhandled promise rejection: detail={:?}",
        "Error: Permission denied: /srv/data/Anna Kovacs/Medical records (and /Users/ada/Downloads/a.pdf)"
    );
    let redacted = context.redact_line(&line).into_owned();
    for private in ["srv/data", "Anna", "Kovacs", "Medical", "records", "ada"] {
        assert!(!redacted.contains(private), "{private:?} survived: {redacted}");
    }
    assert!(redacted.contains("Permission denied: /<dir:"), "{redacted}");
    assert!(
        redacted.contains("$HOME/Downloads/<file:"),
        "the home role survives: {redacted}"
    );
    assert_eq!(context.redact_line(&redacted), redacted, "idempotent");
}
