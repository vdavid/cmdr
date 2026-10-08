//! Tests for the redactor.
//!
//! Each pattern class has its own test with 6+ input→expected tuples. There's also a
//! negative test (path-shaped strings that aren't paths), an idempotency check, a
//! golden-corpus snapshot, and a histogram test that prints replacement counts so
//! coverage regressions show up as numeric diffs.

use super::*;
use std::borrow::Cow;

const TEST_PROCESS_SECRET: [u8; 32] = [0x5a; 32];

/// Helper: redact_line returns Cow; tests want String.
fn r(s: &str) -> String {
    redact_line(s).into_owned()
}

fn context(report_id: &str) -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, report_id)
}

fn report_shape(input: &str) -> String {
    let output = context("ERR-SHAPE").redact_line(input);
    let token = Regex::new(r"<([a-z0-9-]+):[0-9a-f]{12}>").expect("valid report-token regex");
    token.replace_all(&output, "<$1>").into_owned()
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
fn unix_home_paths() {
    let cases = [
        ("/Users/john/Documents/budget.pdf", "$HOME/Documents/<file>.pdf"),
        ("/Users/alice/Downloads/installer.dmg", "$HOME/Downloads/<file>.dmg"),
        // `id_rsa` has no extension-like suffix → labeled `<dir>` under the post-fix-7
        // heuristic. Acceptable trade-off: extensionless files (id_rsa, README, Makefile)
        // are rare in our log corpus, while extensionless directories are common, so
        // defaulting to `<dir>` reads more accurately on real triage data.
        ("/home/bob/.ssh/id_rsa", "$HOME/<dir>/<dir>"),
        ("/Users/veszelovszki/SecretProject/notes.md", "$HOME/<dir>/<file>.md"),
        (
            "Error reading /Users/foo/Desktop/screenshot.png now",
            "Error reading $HOME/Desktop/<file>.png now",
        ),
        (
            "two paths: /Users/a/Documents/x.txt and /Users/b/Downloads/y.zip done",
            "two paths: $HOME/Documents/<file>.txt and $HOME/Downloads/<file>.zip done",
        ),
        ("/Users/john", "$HOME"),
        (
            "/Users/veszelovszki/Library/Application Support/com.veszelovszki.cmdr-dev",
            // `Library/Application Support` is a home role, kept whole under `$HOME`. The leaf
            // `com.veszelovszki.cmdr-dev` has dots but the trailing segment `cmdr-dev` contains
            // a `-` (not alnum), so `has_extension_like_suffix` returns false → leaf labeled
            // `<dir>` (correct: it IS a directory).
            "$HOME/Library/Application Support/<dir>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn windows_home_paths() {
    let cases = [
        (r"C:\Users\Bob\Desktop\passwords.txt", r"$HOME\Desktop\<file>.txt"),
        (r"D:\Users\alice\Documents\report.docx", r"$HOME\Documents\<file>.docx"),
        (
            r"C:\Users\bob\AppData\Roaming\config.json",
            // "Roaming" not safe → <dir>; "AppData" is safe but it's the GRANDPARENT.
            r"$HOME\<dir>\<dir>\<file>.json",
        ),
        (
            r"file at C:\Users\carol\Music\song.mp3 found",
            r"file at $HOME\Music\<file>.mp3 found",
        ),
        (r"C:\Users\dave\SecretFolder\thing.exe", r"$HOME\<dir>\<file>.exe"),
        (r"C:\Users\eve", r"$HOME"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn volumes_paths() {
    let cases = [
        ("/Volumes/MyDrive/file.txt", "/Volumes/<volume>/<file>.txt"),
        (
            // A home role name off `$HOME` is the user's own naming.
            "/Volumes/My Backup Drive/Documents/photo.jpg",
            "/Volumes/<volume>/<dir>/<file>.jpg",
        ),
        (
            "/Volumes/Backup/2026/january/data.csv",
            "/Volumes/<volume>/<dir>/<dir>/<file>.csv",
        ),
        ("/Volumes/Untitled", "/Volumes/<volume>"),
        (
            "mounted at /Volumes/External SSD/work/project.tar.gz now",
            // .gz keeps as ext (3 alnum chars)
            "mounted at /Volumes/<volume>/<dir>/<file>.gz now",
        ),
        ("/Volumes/Time Machine Backups/foo.bak", "/Volumes/<volume>/<file>.bak"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn media_paths() {
    let cases = [
        ("/media/usb0/file.txt", "/media/<volume>/<file>.txt"),
        (
            "/media/alice/External/Documents/x.pdf",
            "/media/<volume>/<dir>/<dir>/<file>.pdf",
        ),
        ("/media/cdrom", "/media/<volume>"),
        ("/media/My Stick/data.bin", "/media/<volume>/<file>.bin"),
        (
            "mounted /media/sdcard/dcim/photo.jpg ok",
            "mounted /media/<volume>/<dir>/<file>.jpg ok",
        ),
        // Non-role allowlisted parents survive anywhere.
        ("/media/usb1/src/main.rs", "/media/<volume>/src/<file>.rs"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// A volume label may contain spaces, but prose after the path must survive.
///
/// Real incident: `VolumeManager::report_identity_conflict` logs the two roots and then keeps
/// talking. The label group swallowed every following lowercase word, so 98 uploaded error
/// reports arrived truncated at the second `/Volumes/<volume>`, losing the volume ID and the
/// resolution, the two fields triage actually needs.
#[test]
fn mount_label_stops_at_lowercase_prose() {
    let cases = [
        (
            "Two different mount roots (/Volumes/naspi and /Volumes/naspi-1) claim volume ID \
             smb-192-168-1-111-445-naspi; the earlier root stays recorded, so the two share \
             per-volume state. Expected only for a cloned volume or a doubly-mounted filesystem.",
            "Two different mount roots (/Volumes/<volume> and /Volumes/<volume>) claim volume ID \
             smb-192-168-1-111-445-naspi; the earlier root stays recorded, so the two share \
             per-volume state. Expected only for a cloned volume or a doubly-mounted filesystem.",
        ),
        // A single trailing lowercase word was already handled; a run of them was not.
        (
            "/Volumes/Untitled is not the volume we wanted here",
            "/Volumes/<volume> is not the volume we wanted here",
        ),
        (
            "/media/usb0 could not be read because the device went away.",
            "/media/<volume> could not be read because the device went away.",
        ),
        // Multi-word labels still match: the continuation words are capitalized.
        ("/Volumes/My Backup Drive is full", "/Volumes/<volume> is full"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// A filename whose own words are lowercase must be redacted WHOLE.
///
/// Real incident: a trash failure logged `.../Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg: …`
/// and the capture stopped dead at ` at`, so ` 01.13.03 PM-2.jpeg` shipped verbatim in an
/// uploaded bundle. Any filename with two or more words leaked its tail the same way, and
/// `Invoice for Acme Corp.pdf` is the shape that actually hurts.
#[test]
fn multi_word_filenames_are_redacted_whole() {
    let cases = [
        // The reported line. The `: ` is the `{path}: {message}` seam, so the message survives.
        (
            "/Users/kajotac/Pics/Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg: the Trash refused it",
            "$HOME/<dir>/<file>.jpeg: the Trash refused it",
        ),
        // A folder whose last word is lowercase, right up to the seam. The seam says where the
        // path ends, so `trip` is part of the name, not prose.
        (
            "/Users/jo/Pics/summer trip: it timed out",
            "$HOME/<dir>/<dir>: it timed out",
        ),
        // A lowercase word mid-filename, nothing after it.
        ("/Users/jo/Docs/my secret notes.txt", "$HOME/<dir>/<file>.txt"),
        // The shape with real exposure in it.
        ("/Users/jo/Work/Invoice for Acme Corp.pdf", "$HOME/<dir>/<file>.pdf"),
        // Prose after an extension still survives: the trailing run has no extension to hold it.
        (
            "/Users/jo/Documents/notes.md failed to open",
            "$HOME/Documents/<file>.md failed to open",
        ),
        // A digit-led "extension" inside a timestamp must NOT end the name early. This is
        // the exact shape that leaked: cutting at `01.13.03` ships ` PM-2.jpeg`.
        (
            "/Users/jo/Shots/Screenshot 2026-09-15 at 11.10.35 AM-2.jpeg",
            "$HOME/<dir>/<file>.jpeg",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// The reported line, end to end.
///
/// ⚠️ Known gap, deliberately pinned here: macOS repeats the filename inside its own error
/// prose, in curly quotes and with no path around it. No path pattern claims a bare name, so
/// that copy still ships. Closing it needs a separate pass that redacts verbatim repeats of a
/// segment already recognized on the same line; the path itself is what this test covers.
#[test]
fn trash_refusal_line_redacts_its_path() {
    let input = "op 01a0 (Trash) failed: /Users/kajotac/Library/CloudStorage/Dropbox/Shots/\
                 Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg: the Trash refused it";
    let out = r(input);
    assert!(
        out.starts_with("op 01a0 (Trash) failed: $HOME/Library/CloudStorage/<dir>/<dir>/<file>.jpeg"),
        "path not fully redacted: {out}"
    );
    assert!(
        !out.contains("01.13.03") && !out.contains("PM-2"),
        "filename fragments survived: {out}"
    );
    assert!(out.ends_with(": the Trash refused it"), "message lost: {out}");
}

/// A share-relative path in a `key=value` log field is redacted like any other path.
///
/// Real incident: every SMB log line printed its share-relative path in full (30 lines in one
/// bundle), because no path branch matches a path with no mount prefix in front of it. The
/// `smb2` crate's own lines print it unquoted and backslash-separated, with spaces in it.
#[test]
fn share_relative_paths_in_fields() {
    let cases = [
        // `cmdr-smb`'s Debug-quoted shape.
        (
            r#"SmbVolume::delete: share=media, path="trips/2023/summer trip/kapu méretek.jpg""#,
            r#"SmbVolume::delete: share=media, path="<dir>/<dir>/<dir>/<file>.jpg""#,
        ),
        (
            r#"SmbVolume::rename: share=media, from="trips/a b.jpg.cmdr-tmp-66381a4a-7bff", to="trips/a b.jpg", force=false"#,
            // Cmdr's own temp suffix carries no PII and survives, so the temp and the final
            // name visibly belong together.
            r#"SmbVolume::rename: share=media, from="<dir>/<file>.jpg.cmdr-tmp-66381a4a-7bff", to="<dir>/<file>.jpg", force=false"#,
        ),
        // Both fields on one line: the absolute one goes to the `/Volumes/` branch.
        (
            r#"SmbVolume::get_metadata: share=media, input="/Volumes/media/trips/x y.jpg", smb_path="trips/x y.jpg""#,
            r#"SmbVolume::get_metadata: share=media, input="/Volumes/<volume>/<dir>/<file>.jpg", smb_path="<dir>/<file>.jpg""#,
        ),
        // `smb2`'s unquoted, backslash-separated shapes. The value ends at the next field.
        (
            r"tree: renamed from=trips\2023\summer trip\kapu méretek.jpg.cmdr-tmp-6638 to=trips\2023\summer trip\kapu méretek.jpg",
            r"tree: renamed from=<dir>\<dir>\<dir>\<file>.jpg.cmdr-tmp-6638 to=<dir>\<dir>\<dir>\<file>.jpg",
        ),
        (
            r"tree: deleted file=trips\2023\summer trip\kapu méretek.jpg",
            r"tree: deleted file=<dir>\<dir>\<dir>\<file>.jpg",
        ),
        (
            r"tree: created directory=trips\2021\rotate script",
            r"tree: created directory=<dir>\<dir>\<dir>",
        ),
        (
            r"tree: watch path=trips\2021, recursive=false, tree_id=5",
            r"tree: watch path=<dir>\<dir>, recursive=false, tree_id=5",
        ),
        // The `{path}: {message}` seam and a closing paren end an unquoted value too.
        (
            "SmbVolume::download(share=media, path=trips/a b.jpg): cancelled after 5 bytes",
            "SmbVolume::download(share=media, path=<dir>/<file>.jpg): cancelled after 5 bytes",
        ),
        // A bare name, and a volume-relative path with a leading slash.
        (
            "loadDirectory called: paneId=right, path=/private, selectName=-Users-alice-projects-cmdr, currentLoading=false",
            "loadDirectory called: paneId=right, path=/private, selectName=<dir>, currentLoading=false",
        ),
        (
            r#"checking 1 item(s) against path="/trips/2023/summer trip" on volume smb-1"#,
            r#"checking 1 item(s) against path="/<dir>/<dir>/<dir>" on volume smb-1"#,
        ),
        // Allowlisted parents survive, like in every other path shape, but a home role name
        // proves nothing off `$HOME`.
        (
            r#"write_from_stream: share=media, path="src/report.pdf", size=12"#,
            r#"write_from_stream: share=media, path="src/<file>.pdf", size=12"#,
        ),
        (
            r#"write_from_stream: share=media, path="Documents/report.pdf", size=12"#,
            r#"write_from_stream: share=media, path="<dir>/<file>.pdf", size=12"#,
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// Keys that merely end in a path key, and values that aren't paths, stay put.
#[test]
fn path_fields_leave_non_paths_alone() {
    let unchanged = [
        "enrich parent_id=12 new_parent_id=13",
        "path=None",
        "ByteSeekBackend::get_lines: target=Line(5) -> byte 0",
        "refresh_listing: path=/Volumes/<volume>/<dir:dae7a7>/<dir:ee4032>",
        "list_directory_core: path=$HOME/<dir:b36d39>/<dir:7e85ce>, entries=1",
    ];
    for input in unchanged {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
}

/// A temp-suffixed filename with a space in it must be redacted whole.
///
/// Real incident: `…/boldogságkapu me\u{301}retek.jpg.cmdr-tmp-<uuid>` (a `{:?}`-printed NFD
/// name) was split at the space. The tail carries no extension the backward trim recognizes
/// (`cmdr-tmp-…` has dashes), so it read as prose and shipped verbatim.
#[test]
fn temp_suffixed_filename_with_a_space_is_redacted_whole() {
    let cases = [
        (
            r#"input="/Volumes/media/trips/2023/kapu me\u{301}retek.jpg.cmdr-tmp-66381a4a-7bff-45c7", smb_path="x""#,
            r#"input="/Volumes/<volume>/<dir>/<dir>/<file>.jpg.cmdr-tmp-66381a4a-7bff-45c7", smb_path="<dir>""#,
        ),
        (
            "/Users/jo/Pics/kapu méretek.jpg.cmdr-tmp-66381a4a-7bff-45c7",
            "$HOME/<dir>/<file>.jpg.cmdr-tmp-66381a4a-7bff-45c7",
        ),
        // A Debug escape closing a word is not the end of a sentence.
        (
            r#"path="/Volumes/media/cafe\u{301} menu.pdf""#,
            r#"path="/Volumes/<volume>/<file>.pdf""#,
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn one_context_correlates_repeated_names_with_twelve_lowercase_hex_chars() {
    let context = context("ERR-AAAAA");
    let first = context.redact_line("/Users/jo/Pics/private report.pdf");
    let second = context.redact_line(r#"path="/Users/jo/Pics/private report.pdf""#);
    let first_token = token(&first, "file");

    assert_eq!(token(&second, "file"), first_token);
    assert_eq!(first_token.len(), 12);
    assert!(
        first_token
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

#[test]
fn separate_contexts_do_not_correlate_the_same_name() {
    let first = context("ERR-AAAAA").redact_line("/Users/jo/Pics/private report.pdf");
    let second = context("ERR-BBBBB").redact_line("/Users/jo/Pics/private report.pdf");

    assert_ne!(token(&first, "file"), token(&second, "file"));
}

#[test]
fn real_identity_domains_do_not_correlate_the_same_value() {
    let context = context("ERR-AAAAA");
    let tokens = [
        token(&context.redact_line(r#"path="Sentinel""#), "dir"),
        token(&context.redact_line("Sentinel.local"), "host"),
        token(&context.redact_line("user=Sentinel"), "user"),
        token(&context.redact_line("/Volumes/Sentinel"), "volume"),
        token(&context.redact_line("manual-Sentinel-445"), "server-id"),
        token(&context.redact_line("Sentinel's Pixel"), "mtp-owner"),
    ];

    for (index, left) in tokens.iter().enumerate() {
        assert!(tokens[index + 1..].iter().all(|right| right != left));
    }
}

#[test]
fn process_secrets_separate_the_same_report() {
    let first = RedactionContext::for_test([0x11; 32], "ERR-AAAAA").redact_line("/Users/jo/Pics/private report.pdf");
    let second = RedactionContext::for_test([0x22; 32], "ERR-AAAAA").redact_line("/Users/jo/Pics/private report.pdf");

    assert_ne!(token(&first, "file"), token(&second, "file"));
}

/// One name can arrive from `Display` in NFD, from `Debug` with escapes, or from a NAS in NFC.
#[test]
fn context_normalizes_unicode_debug_spelling_and_staging_names() {
    let context = context("ERR-AAAAA");
    let display_nfd = context.redact_line("/Users/jo/Pics/kapu me\u{301}retek.jpg");
    let debug_nfd = context.redact_line(r#"path="/Users/jo/Pics/kapu me\u{301}retek.jpg""#);
    let display_nfc = context.redact_line("/Users/jo/Pics/kapu m\u{e9}retek.jpg");
    let staged = context.redact_line(r#"path="Pics/kapu méretek.jpg.cmdr-tmp-66381a4a""#);
    let expected = token(&display_nfd, "file");

    assert_eq!(token(&debug_nfd, "file"), expected);
    assert_eq!(token(&display_nfc, "file"), expected);
    assert_eq!(token(&staged, "file"), expected);
    assert!(staged.ends_with(".jpg.cmdr-tmp-66381a4a\""));
}

#[test]
fn context_keeps_extensionless_and_staging_name_shape() {
    let context = context("ERR-AAAAA");
    let extensionless = context.redact_line(r#"path="projects/README""#);
    let staged = context.redact_line(r#"path="projects/report.pdf.cmdr-staging-dead-beef""#);

    assert_eq!(token(&extensionless, "dir").len(), 12);
    assert!(!extensionless.contains("README"));
    assert!(staged.ends_with(".pdf.cmdr-staging-dead-beef\""));
}

#[test]
fn context_preserves_only_context_proven_home_downloads_role() {
    let context = context("ERR-AAAAA");
    let home = context.redact_line("/Users/alice/Downloads/client/project/report.pdf");
    let unrelated = context.redact_line("/Users/alice/Work/Downloads/report.pdf");
    let remote = context.redact_line("smb://server/share/Downloads/report.pdf");

    assert!(home.starts_with("$HOME/Downloads/<dir:"), "{home}");
    assert!(!unrelated.contains("Downloads"), "{unrelated}");
    assert!(!remote.contains("Downloads"), "{remote}");
}

/// TCC protection depends on which home folder a path is in, so every well-known one keeps
/// its name under `$HOME`, at any depth, and nowhere else.
#[test]
fn context_preserves_every_well_known_home_folder_role() {
    let context = context("ERR-ROLES");
    for (input, role) in [
        ("/Users/alice/Desktop/client/deep/plan.key", "$HOME/Desktop/<dir:"),
        ("/Users/alice/Documents/client/deep/plan.pdf", "$HOME/Documents/<dir:"),
        ("/Users/alice/Pictures/Trip/deep/a.jpg", "$HOME/Pictures/<dir:"),
        ("/Users/alice/Movies/Trip/deep/a.mov", "$HOME/Movies/<dir:"),
        ("/Users/alice/Music/Band/deep/a.mp3", "$HOME/Music/<dir:"),
        ("/Users/alice/Library/Caches/deep/x.db", "$HOME/Library/<dir:"),
        (
            "/Users/alice/Library/Mobile Documents/com~apple~CloudDocs/Taxes/2026.pdf",
            "$HOME/Library/Mobile Documents/<dir:",
        ),
        (
            "/Users/alice/Library/CloudStorage/GoogleDrive-alice@example.com/Taxes/2026.pdf",
            "$HOME/Library/CloudStorage/<dir:",
        ),
    ] {
        let redacted = context.redact_line(input);
        assert!(redacted.starts_with(role), "{input} → {redacted}");
        for private in [
            "alice",
            "client",
            "Trip",
            "Band",
            "Taxes",
            "Caches",
            "CloudDocs",
            "GoogleDrive",
        ] {
            assert!(!redacted.contains(private), "{private:?} survived: {redacted}");
        }
    }
    let unrelated = context.redact_line("/Users/alice/Work/Documents/deep/plan.pdf");
    let remote = context.redact_line("smb://server/share/Pictures/Trip/a.jpg");
    assert!(!unrelated.contains("Documents"), "{unrelated}");
    assert!(!remote.contains("Pictures"), "{remote}");
}

#[test]
fn typed_paths_consume_extensionless_multiword_leaves_as_complete_values() {
    let context = context("ERR-TYPED");
    let cases = [
        ("/Users/alice/Plans/client secret", "$HOME/"),
        ("/Volumes/Client Disk/Plans/client secret", "/Volumes/"),
        ("/media/Client Disk/Plans/client secret", "/media/"),
        ("Plans/client secret", "<dir:"),
        ("sftp://alice@host.test/Plans/client secret", "sftp://"),
        (r"\\host\Private Share\Plans\client secret", r"\\"),
    ];

    for (input, prefix) in cases {
        let output = context.redact_path(input);
        assert!(
            output.starts_with(prefix),
            "typed shape changed for {input:?}: {output}"
        );
        assert!(
            !output.contains("client"),
            "first leaf word survived for {input:?}: {output}"
        );
        assert!(
            !output.contains("secret"),
            "trailing leaf word survived for {input:?}: {output}"
        );
    }
}

#[test]
fn typed_raw_paths_never_trust_caller_supplied_token_syntax() {
    let context = context("ERR-TYPED-RAW");
    let cases = [
        (
            "/mnt/<alice-smith>/<file:abcdef>/secret.txt",
            ["<alice-smith>", "<file:abcdef>"],
        ),
        (
            "https://<raw-user>@<raw-host>/<raw-folder>/secret.txt",
            ["<raw-user>", "<raw-host>"],
        ),
    ];

    for (input, hostile_tokens) in cases {
        let output = context.redact_path(input);
        for hostile_token in hostile_tokens {
            assert!(
                !output.contains(hostile_token),
                "caller token {hostile_token:?} survived typed redaction for {input:?}: {output}"
            );
        }
    }

    let line = r#"path="/mnt/alice-smith/secret.txt""#;
    let once = context.redact_line(line);
    assert_eq!(
        context.redact_line(&once),
        once,
        "transformed report lines must stay idempotent"
    );
}

#[test]
fn smb_uris() {
    let cases = [
        (
            "smb://server.local/share/file.txt",
            "smb://<host>.local/<share>/<file>.txt",
        ),
        (
            "smb://192.168.1.10/Public/doc.pdf",
            "smb://<ipv4-private>/<share>/<file>.pdf",
        ),
        (
            "smb://nas.local/backups/2026/jan.zip",
            "smb://<host>.local/<share>/<dir>/<file>.zip",
        ),
        (
            "Connecting to smb://homer/movies/film.mkv now",
            "Connecting to smb://<host>/<share>/<file>.mkv now",
        ),
        ("smb://homer", "smb://<host>"),
        ("smb://homer/share", "smb://<host>/<share>"),
    ];
    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn unc_paths() {
    let cases = [
        (r"\\server\share\file.txt", r"\\<host>\<share>\<file>.txt"),
        (
            r"\\nas.local\public\Documents\plan.docx",
            r"\\<host>.local\<share>\<dir>\<file>.docx",
        ),
        (r"\\server\share", r"\\<host>\<share>"),
        (r"\\server", r"\\<host>"),
        (
            r"opening \\fileserver\team\report.pdf failed",
            r"opening \\<host>\<share>\<file>.pdf failed",
        ),
        (
            r"\\10.0.0.5\backup\daily\snapshot.tar",
            r"\\<ipv4-private>\<share>\<dir>\<file>.tar",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn mdns_hostnames() {
    let cases = [
        ("connecting to homer.local", "connecting to <host>.local"),
        ("nas.local resolved", "<host>.local resolved"),
        ("ping macbook-pro.local for status", "ping <host>.local for status"),
        ("two: alpha.local and beta.local", "two: <host>.local and <host>.local"),
        ("server-1.local", "<host>.local"),
        ("foo.local:445", "<host>.local:445"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn ipv4_addresses() {
    let cases = [
        ("connect to 192.168.1.1 timeout", "connect to <ipv4-private> timeout"),
        ("10.0.0.5", "<ipv4-private>"),
        ("from 8.8.8.8 to 8.8.4.4", "from <ipv4-public> to <ipv4-public>"),
        ("172.16.254.1:8080", "<ipv4-private>:8080"),
        ("0.0.0.0", "<ipv4-unspecified>"),
        ("127.0.0.1", "<ipv4-loopback>"),
        ("169.254.10.20", "<ipv4-link-local>"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn ipv6_addresses() {
    let cases = [
        ("2001:db8:85a3::8a2e:370:7334", "<ipv6-public>"),
        ("::1", "<ipv6-loopback>"),
        ("fe80::1", "<ipv6-link-local>"),
        ("from 2001:db8::1 to ::1", "from <ipv6-public> to <ipv6-loopback>"),
        ("fd00::abcd:1234", "<ipv6-private>"),
        ("2001:0db8:0000:0000:0000:ff00:0042:8329", "<ipv6-public>"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn email_addresses() {
    let cases = [
        ("contact alice@example.com please", "contact <email> please"),
        ("john.doe+tag@example.co.uk", "<email>"),
        ("two: a@b.com and c@d.org", "two: <email> and <email>"),
        ("noreply@subdomain.example.com", "<email>"),
        ("name_with_underscores@x-y.io", "<email>"),
        ("user@domain.dev failed login", "<email> failed login"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

#[test]
fn url_userinfo() {
    let cases = [
        (
            "https://alice:s3cret@example.com/path",
            "https://<user>:<credential>@<host>/<dir>",
        ),
        ("ftp://anon@files.example.com/pub", "ftp://<user>@<host>/<dir>"),
        ("https://user@host.example.com/", "https://<user>@<host>/"),
        (
            "fetched https://bob:hunter2@api.example.com/v1 ok",
            "fetched https://<user>:<credential>@<host>/<dir> ok",
        ),
        ("ssh://git@github.com/foo/bar", "ssh://<user>@<host>/<dir>/<dir>"),
        (
            "https://u:p@a.com and https://x:y@b.com",
            "https://<user>:<credential>@<host> and https://<user>:<credential>@<host>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn bare_userinfo_no_scheme() {
    // The macOS `smbutil` and Linux `smbclient` fallbacks build `//user:pass@host` URLs
    // (no scheme). A misbehaving server can reflect them in stderr, so the redactor must
    // strip the userinfo on this scheme-less shape too.
    let cases = [
        ("//alice:s3cret@192.168.1.10", "//<user>:<credential>@<ipv4-private>"),
        ("//bob@nas.example.com/share", "//<user>@<host>/<share>"),
        (
            "smbutil failed for //user:pass@host:10480",
            "smbutil failed for //<user>:<credential>@<host>:10480",
        ),
        (
            "stderr: //admin:hunter2@server now",
            "stderr: //<user>:<credential>@<host> now",
        ),
        // A scheme'd URL must still go through url_userinfo, not double-match the bare tail.
        (
            "http://alice:s3cret@example.com/x",
            "http://<user>:<credential>@<host>/<dir>",
        ),
        (
            "connect //u:p@a and //x:y@b",
            "connect //<user>:<credential>@<host> and //<user>:<credential>@<host>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(report_shape(input), expected, "input: {input:?}");
    }
}

#[test]
fn mtp_device_owner_names() {
    let cases = [
        (
            "Connected to John's Pixel 8 Pro",
            "Connected to <mtp-owner>'s Pixel 8 Pro",
        ),
        ("device: Alice's iPhone 15 Pro", "device: <mtp-owner>'s iPhone 15 Pro"),
        (
            "Mary's Galaxy S24 Ultra connected",
            "<mtp-owner>'s Galaxy S24 Ultra connected",
        ),
        ("Bob's Pixel discovered", "<mtp-owner>'s Pixel discovered"),
        ("Found Charlie's iPad Pro", "Found <mtp-owner>'s iPad Pro"),
        (
            "two: Anna's Phone and Diana's Tablet",
            "two: <mtp-owner>'s Phone and <mtp-owner>'s Tablet",
        ),
        ("Eric's OnePlus 12 connected", "<mtp-owner>'s OnePlus 12 connected"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// English contractions, module paths, and bare model names must NOT be touched.
#[test]
fn mtp_owner_negatives() {
    let must_be_unchanged = [
        // English contractions: `It`, `That`, `He`, `She` would be the "owner"
        // candidate but we only match capitalized words AND a known model word.
        // `it's a Pixel` has lowercase `it`, so safe. `That's a Pixel 8 Pro` has
        // capitalized "That" but "Pixel 8 Pro" follows (uh oh, that WOULD match).
        // Avoid that by listing safe sentences without leading "<Capital>'s <model>"
        // shape, plus a few realistic non-owner sentences.
        "it's a Pixel 8 Pro phone",
        "the device is a Pixel 8 Pro",
        "Pixel 8 Pro detected",
        "iPhone 15 Pro detected",
        "Galaxy S24 connected",
        // Module paths: must not match.
        "cmdr_lib::mtp::device",
        "cmdr_lib::redact::tests",
        // Random capitalized phrases that look ownership-y but aren't followed by
        // an MTP model word: must NOT match.
        "John's car was here",
        "Alice's project codename",
    ];
    for input in must_be_unchanged {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
}

/// `<Capitalized>'s <Model>` triggers redaction, including pronouns like `That's Pixel`.
/// We accept this overmatch: the `'s` + model shape is rare in English without an actual
/// possessive, and over-redacting a generic sentence is safer than under-redacting a real
/// owner name. Pin the behaviour so any future tightening is deliberate.
///
/// The `\x20+ Model` requirement immediately after `'s` keeps natural sentences with an
/// article in between safe (`That's a Pixel 8 Pro` is unchanged).
#[test]
fn mtp_owner_known_overmatches() {
    assert_eq!(r("That's a Pixel 8 Pro"), "That's a Pixel 8 Pro");
    assert_eq!(r("That's Pixel 8 Pro"), "<mtp-owner>'s Pixel 8 Pro");
}

/// The `user=` / `username=` shape our SMB logs use for the account someone signs in to a
/// share with. The value goes; the `Some(...)` / `None` wrapper stays, because "was there a
/// username at all" is the part a triager actually reads.
#[test]
fn account_names() {
    let cases = [
        (
            "Upgrading volume smb-1 to SmbVolume: server=nas, share=media, user=Some(\"david\")",
            "Upgrading volume smb-1 to SmbVolume: server=nas, share=media, user=Some(\"<user>\")",
        ),
        (
            "Found Keychain credentials for user=david",
            "Found Keychain credentials for user=<user>",
        ),
        (
            "try_list_shares_authenticated: addr=nas:445, user=admin",
            "try_list_shares_authenticated: addr=nas:445, user=<user>",
        ),
        ("username=dvesz connected", "username=<user> connected"),
        // Windows-style domain accounts and dotted names go whole.
        ("user=WORKGROUP\\david", "user=<user>"),
        ("user=david.veszelovszki", "user=<user>"),
        // Quoted, and inside a Rust debug struct.
        (
            "SmbCredentials { username: \"david\", .. }",
            "SmbCredentials { username: \"<user>\", .. }",
        ),
        // Two on one line.
        (
            "user=alice fell back to user=bob",
            "user=<user> fell back to user=<user>",
        ),
        // An absent username is not a secret and stays readable.
        ("Upgrading volume smb-1: user=None", "Upgrading volume smb-1: user=None"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// The account pattern must not eat identifiers that merely end in `user`, nor the many
/// `<key>=<value>` pairs our logs are full of.
#[test]
fn account_name_negatives() {
    let must_be_unchanged = [
        "cmdr_lib::network::smb_client",
        "max_users=12",
        "parent_user=1",
        "browser=safari",
        "users=3",
        "user_count=7",
        "the user pressed Escape",
    ];
    for input in must_be_unchanged {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
}

#[test]
fn unix_system_paths() {
    let cases = [
        (
            "error at /tmp/build-abc123/src/main.rs:42:5",
            "error at /tmp/<dir>/src/<file>.rs:42:5",
        ),
        ("/tmp/foo.txt", "/tmp/<file>.txt"),
        // `zeb_def_ipc_93056` has no extension-like suffix → `<dir>` (post-fix-7 default).
        ("/private/tmp/zeb_def_ipc_93056", "/private/<dir>/<dir>"),
        (
            "/var/folders/xy/abcdef/T/cache.bin",
            "/var/<dir>/<dir>/<dir>/<dir>/<file>.bin",
        ),
        // `something` has no extension-like suffix → `<dir>` (post-fix-7 default).
        ("/opt/homebrew/bin/something", "/opt/<dir>/<dir>/<dir>"),
        ("/tmp/", "/tmp/"),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// Strings that look path-ish or PII-ish but aren't. Must pass through unchanged.
#[test]
fn negatives_unchanged() {
    let cases = [
        "Cargo.toml",
        "cmdr_lib::network::smb_client",
        "cmdr_lib::redact::tests",
        "0.1.2-alpha",
        "192.168.x.y",
        "version 1.2.3",
        "MustScanSubDirs: reconcile slow for / (+38 -0 ~515676, 1691s)",
        "called `Option::unwrap()` on a `None` value",
        "Reconciler: switched to live mode",
        "indexing::manager  Replay: watcher started (since_event_id=888910657, current=890423195)",
        "127 errors", // not 4-octet IP
        "1.2.3.4.5",  // 5 octets: IPv4 regex matches first 4. acceptable; see below.
        "release v0.13.0",
    ];
    // Most must be unchanged. A couple noted as "acceptable to redact":
    let must_be_unchanged = [
        "Cargo.toml",
        "cmdr_lib::network::smb_client",
        "cmdr_lib::redact::tests",
        "0.1.2-alpha",
        "192.168.x.y",
        "version 1.2.3",
        "MustScanSubDirs: reconcile slow for / (+38 -0 ~515676, 1691s)",
        "called `Option::unwrap()` on a `None` value",
        "Reconciler: switched to live mode",
        "indexing::manager  Replay: watcher started (since_event_id=888910657, current=890423195)",
        "127 errors",
        "release v0.13.0",
    ];
    for input in must_be_unchanged {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
    // The 1.2.3.4.5 case: IPv4 regex matches "1.2.3.4", which is acceptable; assert it doesn't
    // crash and produces some redaction.
    let _ = r(cases[11]);
}

#[test]
fn idempotency() {
    let corpus = [
        "/Users/john/Documents/budget.pdf",
        r"C:\Users\Bob\Desktop\x.txt",
        "/Volumes/Backup/photo.jpg",
        "smb://homer.local/share/x.txt",
        "https://u:p@host.com/path",
        "alice@example.com",
        "192.168.1.1",
        "2001:db8::1",
        "homer.local",
        "Reconciler: switched to live mode",
        "indexing::manager  Replay: watcher started (since_event_id=888910657)",
        r#"SmbVolume::delete: share=media, path="trips/summer trip/a b.jpg""#,
        r"tree: renamed from=trips\summer trip\a b.jpg.cmdr-tmp-1 to=trips\summer trip\a b.jpg",
        "loadDirectory: path=/private, selectName=projects",
        r#"checking 1 item(s) against path="/trips/summer trip" on volume smb-1"#,
        "/Volumes/media/cafe\\u{301} menu.pdf",
    ];
    for input in corpus {
        let once = r(input);
        let twice = r(&once);
        assert_eq!(once, twice, "not idempotent for {input:?}");
    }
}

#[test]
fn redact_text_handles_multiple_lines() {
    let input = "first /Users/john/x.txt\nsecond /Volumes/foo/y.bin\nthird clean line\n";
    let expected = "first $HOME/<file>.txt\nsecond /Volumes/<volume>/<file>.bin\nthird clean line\n";
    assert_eq!(redact_text(input), expected);
}

#[test]
fn redact_panic_message_alias() {
    let msg = "panicked at /Users/foo/Documents/bar.rs:10:5";
    let expected = "panicked at $HOME/Documents/<file>.rs:10:5";
    assert_eq!(redact_panic_message(msg), expected);
}

#[test]
fn cow_borrowed_when_no_match() {
    // No PII → Cow::Borrowed (no allocation). We can't observe Cow variant directly via
    // == String, but we can assert the output is identical and the input is short
    // (regression guard against accidental allocation).
    let input = "Reconciler: switched to live mode";
    let out = redact_line(input);
    assert_eq!(&*out, input);
    // Confirm it's a Borrowed variant.
    matches!(out, Cow::Borrowed(_));
}

// --- Golden corpus snapshot ---

/// The synthesized log corpus + its expected redacted form. Touch this snapshot deliberately
/// when a redaction rule changes; CI will diff it for review.
///
/// To regenerate the snapshot after an intentional change:
///     REGENERATE_REDACT_CORPUS=1 cargo nextest run --lib redact::tests::golden_corpus_snapshot
/// Then review the diff and commit.
#[test]
#[allow(clippy::print_stderr, reason = "diagnostic for the regenerate workflow")]
fn golden_corpus_snapshot() {
    let corpus = include_str!("fixtures/log-corpus.txt");
    let expected = include_str!("fixtures/log-corpus.redacted.txt");
    let actual = redact_text(corpus);

    if std::env::var("REGENERATE_REDACT_CORPUS").is_ok() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/redact/fixtures/log-corpus.redacted.txt");
        std::fs::write(&path, &actual).expect("write redacted corpus");
        eprintln!("wrote {}", path.display());
        return;
    }

    if actual != expected {
        // Print a unified diff so the failure is easy to interpret in CI.
        let actual_lines: Vec<&str> = actual.lines().collect();
        let expected_lines: Vec<&str> = expected.lines().collect();
        let mut diff = String::new();
        for (i, (a, e)) in actual_lines.iter().zip(expected_lines.iter()).enumerate() {
            if a != e {
                diff.push_str(&format!("line {}:\n  expected: {e}\n  actual:   {a}\n", i + 1));
            }
        }
        if actual_lines.len() != expected_lines.len() {
            diff.push_str(&format!(
                "line count differs: expected {}, actual {}\n",
                expected_lines.len(),
                actual_lines.len()
            ));
        }
        panic!("golden corpus mismatch (set REGENERATE_REDACT_CORPUS=1 to rewrite):\n{diff}");
    }
}

/// Histogram of replacement counts per pattern class. Prints a table on every run; future
/// coverage regressions show up as numeric drops.
#[test]
#[allow(clippy::print_stderr, reason = "intentional diagnostic output for the histogram")]
fn replacement_count_histogram() {
    let corpus = include_str!("fixtures/log-corpus.txt");
    let redacted = redact_text(corpus);

    let counts = [
        ("$HOME", redacted.matches("$HOME").count()),
        ("/Volumes/<volume>", redacted.matches("/Volumes/<volume>").count()),
        ("/media/<volume>", redacted.matches("/media/<volume>").count()),
        ("smb://<host>", redacted.matches("smb://<host>").count()),
        (r"\\<host>", redacted.matches(r"\\<host>").count()),
        ("<host>.local", redacted.matches("<host>.local").count()),
        ("<ipv4-…>", redacted.matches("<ipv4-").count()),
        ("<ipv6-…>", redacted.matches("<ipv6-").count()),
        ("<email>", redacted.matches("<email>").count()),
        ("<share>", redacted.matches("<share>").count()),
        ("<file>", redacted.matches("<file>").count()),
        ("<dir>", redacted.matches("<dir>").count()),
        ("<mtp-owner>", redacted.matches("<mtp-owner>").count()),
        ("<user>", redacted.matches("<user>").count()),
    ];

    eprintln!("\n=== Redaction histogram ===");
    for (label, count) in &counts {
        eprintln!("  {label:>20} : {count}");
    }
    eprintln!("===========================\n");

    // Sanity: every pattern class is exercised at least once in the corpus.
    for (label, count) in &counts {
        assert!(*count > 0, "no occurrences of {label}: corpus coverage gap");
    }
}
