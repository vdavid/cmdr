//! Synthetic privacy matrix for remote references and name-derived diagnostic IDs.

use super::*;
use cmdr_fs::volume::{
    adb_volume_id, local_volume_id, mtp_ids, path_volume_id, s3_volume_id, sftp_volume_id, smb_volume_id,
    webdav_volume_id,
};

const TEST_PROCESS_SECRET: [u8; 32] = [0x7c; 32];

fn r(input: &str) -> String {
    redact_line(input).into_owned()
}

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-REMOTE")
}

fn report_shape(input: &str) -> String {
    let output = context().redact_line(input);
    let token = Regex::new(r"<([a-z0-9-]+):[0-9a-f]{12}>").expect("valid report-token regex");
    token.replace_all(&output, "<$1>").into_owned()
}

/// `redact_line` serves ordinary MCP resources with the SAME policy as a report, only with bare
/// tokens: complete remote references, structured identities, and name-derived IDs included.
/// A second, weaker policy for MCP once let a space-containing URL swallow the path after it.
#[test]
fn unsalted_api_runs_the_report_policy_with_bare_tokens() {
    let cases = [
        ("/Users/alice/Secret Project/report.pdf", "$HOME/<dir>/<file>.pdf"),
        (
            "smb://ada:secret@nas.local:1445/Finance/Downloads/report.pdf?token=secret#customer",
            "smb://<user>:<credential>@<host>.local:1445/<share>/<dir>/<file>.pdf?<query>=<query>#<fragment>",
        ),
        (
            r"\\nas.local\Finance\Downloads\report.pdf",
            r"\\<host>.local\<share>\<dir>\<file>.pdf",
        ),
        (
            "sftp://ada:secret@files.example.test:2222/home/ada/Client/report.pdf?token=secret#customer",
            "sftp://<user>:<credential>@<host>:2222/<dir>/<dir>/<dir>/<file>.pdf?<query>=<query>#<fragment>",
        ),
        // A URL path may contain spaces, so the prose after it rides along as path segments:
        // over-redacted, never leaked.
        (
            "https://u@host.example/a then /Users/alice/secret.txt",
            "https://<user>@<host>/<dir>/<dir>/<dir>/<file>.txt",
        ),
        (
            "webdav://nas.local/dav/ada/report.pdf?owner=ada@example.test#customer",
            "webdav://<host>.local/<dir>/<dir>/<file>.pdf?<query>=<query>#<fragment>",
        ),
        (
            "https://10.24.8.3/customer/acme/report.json?owner=ada@example.test#customer",
            "https://<ipv4-private>/<dir>/<dir>/<file>.json?<query>=<query>#<fragment>",
        ),
        (
            r#"host="Client Nimbus" share="Private Vault""#,
            r#"host="<host>" share="<share>""#,
        ),
        (r#"host="nas.local" user="ada""#, r#"host="<host>.local" user="<user>""#),
        (
            "IDs smb-nas-private-445-client-0123456789abcdef manual-192-168-40-9-1445",
            "IDs smb-<volume-id> manual-<server-id>-1445",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(r(input), expected, "unsalted output for {input:?}");
        assert_eq!(r(input), report_shape(input), "the two policies split for {input:?}");
    }
}

/// cmdr-reports#30: a URL with a space in its path once swallowed the absolute path after it
/// under the unsalted policy, so the user's home folder name shipped raw.
#[test]
fn a_space_containing_url_never_hides_the_path_after_it() {
    let cases = [
        "fetched https://u@host.example/My Folder/a.txt then /Users/alice/secret.txt",
        "fetched sftp://ada@nas.example/Client Nimbus/report.pdf and /Users/alice/Private Plans/b.pdf",
        "webdav://nas.local/dav/Shared Stuff/x.docx then /Volumes/Alice Backup/notes.md",
    ];
    for input in cases {
        for out in [r(input), report_shape(input)] {
            assert!(!out.contains("alice") && !out.contains("Alice"), "{out}");
            assert!(
                !out.contains("Private") && !out.contains("Nimbus") && !out.contains("Shared"),
                "{out}"
            );
        }
    }
}

/// A production Svelte build throws `Error("https://svelte.dev/e/<code>")` and nothing else, so
/// tokenizing that URL erased the only clue an uncaught frontend error carries (ERR-DAN3Q's
/// `each_key_duplicate` took a rebuild and a stack-offset lookup to name). Only the exact public
/// shape survives: any userinfo, port, query, fragment, extra segment, or other host is redacted.
#[test]
fn svelte_error_code_urls_survive_but_only_in_their_exact_shape() {
    let kept = [
        r#"ERROR FE:uncaught  Uncaught error at tauri://localhost/_app/immutable/chunks/D6pBjj6a.js:1:14434: detail="Error: https://svelte.dev/e/each_key_duplicate""#,
        "Error: https://svelte.dev/e/effect_update_depth_exceeded.",
        "https://svelte.dev/e/state_unsafe_mutation",
    ];
    for input in kept {
        assert!(r(input).contains("https://svelte.dev/e/"), "unsalted: {:?}", r(input));
        assert_eq!(r(input), report_shape(input), "the two policies split for {input:?}");
        let code = input
            .split("svelte.dev/e/")
            .nth(1)
            .expect("code")
            .trim_end_matches(['"', '.']);
        assert!(context().redact_line(input).contains(code), "report lost {code:?}");
    }

    let redacted = [
        ("https://svelte.dev/e/Alice", "https://<host>/<dir>/<dir>"),
        ("https://svelte.dev/e/code/secret", "https://<host>/<dir>/<dir>/<dir>"),
        (
            "https://svelte.dev/e/code?owner=ada",
            "https://<host>/<dir>/<dir>?<query>=<query>",
        ),
        ("https://svelte.dev/e/code#ada", "https://<host>/<dir>/<dir>#<fragment>"),
        ("https://ada@svelte.dev/e/code", "https://<user>@<host>/<dir>/<dir>"),
        ("https://svelte.dev:8443/e/code", "https://<host>:8443/<dir>/<dir>"),
        ("https://svelte.dev.evil.test/e/code", "https://<host>/<dir>/<dir>"),
        ("http://svelte.dev/e/code", "http://<host>/<dir>/<dir>"),
        ("https://svelte.dev/docs/code", "https://<host>/<dir>/<dir>"),
    ];
    for (input, expected) in redacted {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

fn token(output: &str, kind: &str) -> String {
    let prefix = format!("<{kind}:");
    output
        .split(&prefix)
        .nth(1)
        .and_then(|tail| tail.split('>').next())
        .unwrap_or_else(|| panic!("missing {kind} token in {output:?}"))
        .to_string()
}

#[test]
fn complete_remote_urls_redact_every_identity_but_keep_diagnostic_shape() {
    let cases = [
        (
            "sftp://alice@files.example.test:2222/home/alice/Client Nimbus/report.final.pdf",
            "sftp://<user>@<host>:2222/<dir>/<dir>/<dir>/<file>.pdf",
        ),
        (
            "ssh://deploy:p%40ss@[fd12:3456::8]:22/srv/releases/app.tar.gz",
            "ssh://<user>:<credential>@[<ipv6-private>]:22/<dir>/<dir>/<file>.gz",
        ),
        (
            "webdav://ada@nas.local:8443/remote.php/dav/files/ada/Secret%20Plan.docx",
            "webdav://<user>@<host>.local:8443/<dir>/<dir>/<dir>/<dir>/<file>.docx",
        ),
        (
            "s3://AKIACLIENTKEY@s3.eu-west-1.amazonaws.com:443/client-photos/Client Nimbus/plan.pdf",
            "s3://<user>@<host>:443/<dir>/<dir>/<file>.pdf",
        ),
        (
            "https://api.private.test:9443/customer/acme/report.json?access_token=s3cret&folder=Client%20Nimbus#invoice-42",
            "https://<host>:9443/<dir>/<dir>/<file>.json?<query>=<query>&<query>=<query>#<fragment>",
        ),
        (
            "http://10.24.8.3:8080/dav/%E7%A7%98%E5%AF%86/photo.JPEG",
            "http://<ipv4-private>:8080/<dir>/<dir>/<file>.JPEG",
        ),
        (
            "smb://WORKGROUP%5Calice:hunter2@192.168.40.9:445/Client%20Share/2026/budget.xlsx",
            "smb://<user>:<credential>@<ipv4-private>:445/<share>/<dir>/<file>.xlsx",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn remote_urls_without_userinfo_and_inside_quotes_are_complete() {
    let cases = [
        (
            r#"base_url="https://dav.example.test/dav/Client%20A/notes.txt?sig=private#mine""#,
            r#"base_url="https://<host>/<dir>/<dir>/<file>.txt?<query>=<query>#<fragment>""#,
        ),
        (
            "opening 'sftp://server.example.test:22/home/jo/README' now",
            "opening 'sftp://<host>:22/<dir>/<dir>/<dir>' now",
        ),
        (
            "smb://nas.example.test:1445/Finance/Quarter%20One/forecast.csv",
            "smb://<host>:1445/<share>/<dir>/<file>.csv",
        ),
        (
            "webdav://[2001:db8::44]:443/dav/team/file.txt",
            "webdav://[<ipv6-public>]:443/<dir>/<dir>/<file>.txt",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn unc_scheme_less_and_malformed_but_recognizable_references_are_safe() {
    let cases = [
        (
            r"\\files.example.test\Client Share\Secret Folder\plan.docx",
            r"\\<host>\<share>\<dir>\<file>.docx",
        ),
        (
            r"\\10.0.0.8\Finance\2026\budget.xlsx",
            r"\\<ipv4-private>\<share>\<dir>\<file>.xlsx",
        ),
        (
            "fallback //alice:p%40ss@10.0.0.8:445/Finance/plan.pdf",
            "fallback //<user>:<credential>@<ipv4-private>:445/<share>/<file>.pdf",
        ),
        (
            "webdav://alice:secret@nas%ZZ.local:8443/a%ZZ/report.pdf?token=private#customer",
            "webdav://<user>:<credential>@<host>.local:8443/<dir>/<file>.pdf?<query>=<query>#<fragment>",
        ),
        (
            "https://alice:secret@[not-an-ipv6/Client Nimbus/private.txt?token=private#customer",
            "https://<user>:<credential>@[<host>/<dir>/<file>.txt?<query>=<query>#<fragment>",
        ),
        (
            "sftp://alice@:22/home/alice/private.txt",
            "sftp://<user>@:22/<dir>/<dir>/<file>.txt",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn report_context_correlates_remote_entities_across_spellings_and_separates_domains() {
    let context = context();
    let url = context
        .redact_line("smb://ada:secret@cafe%CC%81-nas.local:445/Re%CC%81gi%20NAS/cafe%CC%81/report.pdf?owner=ada#ada");
    let host = context.redact_line("café-nas.local");
    let account = context.redact_line("user=ada");
    let unc = context.redact_line(r"\\café-nas.local\Régi NAS\café\report.pdf");

    assert_eq!(token(&url, "host"), token(&host, "host"));
    assert_eq!(token(&url, "user"), token(&account, "user"));
    assert_eq!(token(&url, "share"), token(&unc, "share"));
    assert_eq!(token(&url, "dir"), token(&unc, "dir"));

    let identities = [
        token(&url, "host"),
        token(&url, "user"),
        token(&url, "credential"),
        token(&url, "share"),
        token(&url, "dir"),
        token(&url, "query"),
        token(&url, "fragment"),
    ];
    for (index, left) in identities.iter().enumerate() {
        assert!(identities[index + 1..].iter().all(|right| right != left));
    }
}

#[test]
fn current_name_derived_ids_are_tokenized_only_at_the_diagnostic_boundary() {
    let mtp = mtp_ids::device_id_for(Some("Pixel 8 / Client Nimbus"), 0);
    let cases = [
        (smb_volume_id("nas.private", 445, "Client Share"), "smb-<volume-id>"),
        (sftp_volume_id("nas.private", 22, "ada"), "sftp-<volume-id>"),
        (webdav_volume_id("dav.private", 443, "ada"), "webdav-<volume-id>"),
        (
            s3_volume_id("s3.private", 443, "AKIAKEY", Some("client-photos")),
            "s3-<volume-id>",
        ),
        (adb_volume_id("R58M1/Client"), "adb-<device-id>"),
        (mtp.clone(), "mtp-<device-id>"),
        (mtp_ids::mtp_volume_id(&mtp, 65_537), "mtp-<device-id>:65537"),
        (local_volume_id(Some("A1B2-C3D4"), "/Volumes/Client"), "vol-<volume-id>"),
        (path_volume_id("/Volumes/Client Backup"), "path-<volume-id>"),
        (
            crate::network::manual_servers::generate_server_id("192.168.40.9", 1445),
            "manual-<server-id>-1445",
        ),
        (
            crate::network::manual_servers::generate_server_id("café-nas.local", 445),
            "manual-<server-id>-445",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(report_shape(&input), expected, "input: {input:?}");
    }
}

#[test]
fn producer_owned_identity_fields_redact_only_the_six_quoted_keys() {
    let cases = [
        (r#"host="Client Nimbus""#, r#"host="<host>""#),
        (r#"server=Some("Client Nimbus")"#, r#"server=Some("<host>")"#),
        (r#"share="Private Vault""#, r#"share="<share>""#),
        (r#"volumeId="legacy-private-volume""#, r#"volumeId="<volume-id>""#),
        (r#"serverId="private-server""#, r#"serverId="<server-id>""#),
        (r#"deviceId="private-device""#, r#"deviceId="<device-id>""#),
    ];

    for (input, expected) in cases {
        assert_eq!(
            report_shape(input),
            expected,
            "unexpected identity-field rewrite for {input}"
        );
    }

    let near_matches = r#"hostname="Client Nimbus" host_name="Client Nimbus" share_name="Private Vault" volume_id="private-volume" server_id="private-server" device_id="private-device" name="ordinary prose" id="ordinary-id" host=unquoted"#;
    assert_eq!(context().redact_line(near_matches), near_matches);
}

/// The frontend log bridge renders a volume's display name as `volumeName="…"`; it shares the
/// volume domain with `share=`, so one mount reads as one token wherever it's named.
#[test]
fn volume_name_field_shares_the_volume_token_domain() {
    let context = context();
    let redacted = context.redact_line(r#"FE volumeName="Client Share" share="Client Share""#);
    assert!(!redacted.contains("Client Share"), "{redacted}");
    let tokens: Vec<&str> = redacted.matches("<volume:").collect();
    assert_eq!(tokens.len(), 1, "volumeName gets a volume token: {redacted}");
    let token = |key: &str| {
        let start = redacted.find(&format!("{key}=\"<")).expect("field") + key.len() + 2;
        redacted[start..]
            .split('"')
            .next()
            .unwrap_or_default()
            .split(':')
            .nth(1)
            .map(str::to_string)
    };
    assert_eq!(token("volumeName"), token("share"), "{redacted}");
}

#[test]
fn identity_field_keys_are_narrow_and_keep_stable_facts() {
    let context = context();
    let redacted = context.redact_line(
        r#"source=gio backend=smb host="Client Nimbus" share="Private Vault" error_kind=permission_denied code=13 nt_status=STATUS_ACCESS_DENIED"#,
    );

    assert!(!redacted.contains("Client Nimbus"), "host survived: {redacted}");
    assert!(!redacted.contains("Private Vault"), "share survived: {redacted}");
    for fact in [
        "source=gio",
        "backend=smb",
        "error_kind=permission_denied",
        "code=13",
        "nt_status=STATUS_ACCESS_DENIED",
    ] {
        assert!(redacted.contains(fact), "stable fact {fact:?} was lost: {redacted}");
    }

    let unchanged = context.redact_line(r#"hostname_count=2 share_count=4 server_state=ready name="ordinary prose""#);
    assert_eq!(
        unchanged,
        r#"hostname_count=2 share_count=4 server_state=ready name="ordinary prose""#
    );
}

#[test]
fn debug_escaped_structured_fields_are_consumed_as_complete_values() {
    let context = context();
    let cases = [
        ("host", "host name\"line\nnext"),
        ("server", "serve\\r\t雪"),
        ("share", "share\"name\nnext"),
        ("volumeId", "volume\\id\t雪"),
        ("serverId", "server\"id\nnext"),
        ("deviceId", "device\\id\t雪"),
        ("user", "account\"name\nnext\\part\t雪"),
        ("path", "/Users/alice/Plans/client \"secret\"\nnext\\leaf\t雪"),
    ];

    for (key, value) in cases {
        let line = format!("before {key}={value:?}, after=stable");
        let output = context.redact_line(&line);
        assert!(
            !output.contains("name"),
            "identity fragment survived for {key}: {output}"
        );
        assert!(!output.contains("secret"), "path fragment survived for {key}: {output}");
        assert!(
            !output.contains("next"),
            "escaped newline tail survived for {key}: {output}"
        );
        assert!(!output.contains('雪'), "Unicode fragment survived for {key}: {output}");
        assert!(
            output.ends_with(", after=stable"),
            "field boundary changed for {key}: {output}"
        );
    }
}

#[test]
fn derived_id_tokens_correlate_per_report_but_not_across_identity_domains() {
    let context = context();
    let mtp_device = context.redact_line("mtp-pixel-8-4123456789abcdef");
    let mtp_storage = context.redact_line("mtp-pixel-8-4123456789abcdef:65537");
    let adb = context.redact_line("adb-pixel-8-4123456789abcdef");
    let other_report =
        RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-OTHER").redact_line("mtp-pixel-8-4123456789abcdef");

    assert_eq!(token(&mtp_device, "device-id"), token(&mtp_storage, "device-id"));
    assert_ne!(token(&mtp_device, "device-id"), token(&adb, "device-id"));
    assert_ne!(token(&mtp_device, "device-id"), token(&other_report, "device-id"));
}

#[test]
fn remote_and_id_lookalikes_remain_unchanged() {
    let unchanged = [
        "https_proxy=enabled",
        "smb-client retried",
        "smb-not-a-current-id",
        "sftp-name-0123456789abcde",
        "webdav-name-0123456789ABCDEf",
        "adb-device-0123456789abcdefg",
        "smb-this-slug-is-more-than-twenty-four-characters-0123456789abcdef",
        "mtp-pixel-8-4123456789abcdef:4294967296",
        "manual server listening on 445",
        "manual-process-started",
        "manual-nas-private-99999",
        "mtp-ABC123:65537",
        "volumesclientbackup",
        "// this is a comment, not a remote reference",
    ];

    let context = context();
    for input in unchanged {
        assert_eq!(context.redact_line(input), input, "should be unchanged: {input:?}");
    }
}

#[test]
fn remote_references_and_derived_ids_are_idempotent() {
    let inputs = [
        "sftp://alice:secret@files.private:2222/home/alice/Client Nimbus/report.pdf",
        "webdav://alice@nas%ZZ.local:8443/a%ZZ/report.pdf?token=private#customer",
        r"\\files.private\Client Share\Secret Folder\plan.docx",
        "smb-nas-private-445-client-0123456789abcdef",
        "manual-192-168-40-9-1445",
    ];

    let context = context();
    for input in inputs {
        let once = context.redact_line(input);
        assert_eq!(context.redact_line(&once), once, "not idempotent for {input:?}");
    }
}
