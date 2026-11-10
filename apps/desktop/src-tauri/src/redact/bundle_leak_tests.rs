//! Leak shapes that survived redaction in real error-report bundles (#491), rebuilt with synthetic
//! names: a paren inside a segment, a flattened typed error, a Bonjour instance with spaces, and
//! bare volume and share labels.

use super::*;

const TEST_PROCESS_SECRET: [u8; 32] = [0x3d; 32];

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-LEAKS")
}

fn r(s: &str) -> String {
    redact_line(s).into_owned()
}

/// Asserts no word of `private` reaches `out`, through both redaction APIs.
fn assert_ships_none(input: &str, private: &[&str]) {
    for out in [r(input), context().redact_line(input).into_owned()] {
        for word in private {
            assert!(!out.contains(word), "{word:?} shipped: {out}");
        }
    }
}

/// A `(` in a folder name read as the end of a sentence, so everything after the `)` shipped.
#[test]
fn a_paren_in_a_segment_never_ends_the_path() {
    assert_ships_none(
        "list_directory_core: path=/Users/me/Pictures/Astro/2026/Summer (Lake House) 2026, entries=5",
        &["Lake", "House", "2026", "Astro"],
    );
    assert_ships_none(
        "listing /Users/me/Pictures/Astro/2026/Summer (Lake House) 2026, entries=5",
        &["Lake", "House", "2026", "Astro"],
    );
    assert_eq!(
        r("list_directory_core: path=/Users/me/Pictures/Astro/2026/Summer (Lake House) 2026, entries=5"),
        "list_directory_core: path=$HOME/Pictures/<dir>/<dir>/<dir>, entries=5"
    );
}

/// Both sides of a `{from} -> {to}` arrow are paths, and the arrow stays between them.
#[test]
fn an_arrow_between_two_paths_with_parens_keeps_both_redacted() {
    let input = "copying /Users/me/Pictures/A/B/Summer (Lake House) 2026 -> \
                 /Volumes/Backup/A/B/Summer (Lake House) 2026";
    assert_ships_none(input, &["Lake", "House", "2026", "Backup"]);
    assert_eq!(
        r(input),
        "copying $HOME/Pictures/<dir>/<dir>/<dir> -> /Volumes/<volume>/<dir>/<dir>/<dir>"
    );
}

/// The frontend flattened a typed error into prose: `Path: …; Error type: …`. A `(`-led word
/// stopped the absolute-path walk inside the field, and the rest of the path shipped.
#[test]
fn a_flattened_typed_error_tokenizes_every_segment() {
    let input = "Copy error: detail=\"Path: /Volumes/Backup/A/B/Summer (Lake House) Trip/2026-08-24/Sub/.DS_Store; \
                 Error type: io_error\"";
    assert_ships_none(input, &["Lake", "House", "Trip", "2026-08-24", "Sub/", "Backup"]);
    assert_eq!(
        r(input),
        "Copy error: detail=\"Path: /Volumes/<volume>/<dir>/<dir>/<dir>/<dir>/<dir>/.DS_Store; Error type: io_error\""
    );
    // The same shape under a prefix no dedicated branch knows, in the field and in prose.
    assert_ships_none(
        "Copy error: detail=\"Path: /srv/A/Summer (Some Words) Name/2026-08-24/x.pdf; Error type: io_error\"",
        &["Some", "Words", "Name", "2026-08-24"],
    );
    assert_ships_none(
        "copy failed for /srv/A/Summer (Some Words) Name/2026-08-24/x.pdf: busy",
        &["Some", "Words", "Name", "2026-08-24"],
    );
}

/// `mdns_sd` quotes a Bonjour instance whose device name has spaces, an apostrophe, and parens.
#[test]
fn a_quoted_bonjour_instance_is_claimed_whole() {
    let input = "2026-10-19T10:00:00.000+02:00 WARN  mdns_sd::service_daemon  resolve failed for \
                 name: \"Alex's MacBook Pro (2)._companion-link._tcp.local.\"";
    assert_ships_none(input, &["Alex", "MacBook", "Pro", "(2)"]);
    assert!(
        r(input).ends_with("name: \"<host>._companion-link._tcp.local.\""),
        "{}",
        r(input)
    );
    // One device, one token: the instance correlates with the same name keyed as a host.
    let context = context();
    let instance = context.redact_line(input).into_owned();
    let keyed = context
        .redact_line("Resolved: server=\"Alex's MacBook Pro (2)\"")
        .into_owned();
    let token = |line: &str| {
        line.split("<host:")
            .nth(1)
            .and_then(|t| t.split('>').next())
            .map(String::from)
    };
    assert_eq!(token(&instance), token(&keyed), "{instance} vs {keyed}");
    assert!(token(&instance).is_some(), "{instance}");
}

/// Volume and share labels printed bare (`copy_between_volumes`, `smb2`'s own `tree:` line),
/// and the keyed `volumeName`.
#[test]
fn bare_volume_and_share_labels_are_tokenized() {
    assert_ships_none(
        "copy_between_volumes: source_volume=Macintosh HD, dest_volume=Family Photos, 2 sources",
        &["Macintosh", "Family", "Photos"],
    );
    assert_eq!(
        r("copy_between_volumes: source_volume=Macintosh HD, dest_volume=Family Photos, 2 sources"),
        "copy_between_volumes: source_volume=<volume>, dest_volume=<volume>, 2 sources"
    );
    assert_eq!(
        r("tree: connected share=Family Photos, tree_id=7"),
        "tree: connected share=<share>, tree_id=7"
    );
    assert_eq!(
        r("volume outage: volumeName=\"Family Photos\" went away"),
        "volume outage: volumeName=\"<volume>\" went away"
    );
    // A label is one identity whatever its key: the share and the volume it mounts as correlate.
    let context = context();
    let share = context
        .redact_line("tree: connected share=Family Photos, tree_id=7")
        .into_owned();
    let volume = context
        .redact_line("volume outage: volumeName=\"Family Photos\" went away")
        .into_owned();
    let hash = |line: &str| {
        line.split(':')
            .nth(2)
            .and_then(|t| t.split('>').next())
            .map(String::from)
    };
    assert_eq!(hash(&share), hash(&volume), "{share} vs {volume}");
}

/// A path logged as a keyed field keeps the parenthesized stat after it.
#[test]
fn a_keyed_path_keeps_the_stat_after_it() {
    assert_eq!(
        r("Verifier: scanned new dir path=\"/x/y/z\" (5 entries, 0ms)"),
        "Verifier: scanned new dir path=\"/<dir>/<dir>/<dir>\" (5 entries, 0ms)"
    );
    assert_eq!(
        r("MustScanSubDirs: reconcile complete for path=\"/x/y\" (+0 -0 ~0, 6ms, 1ms CPU)"),
        "MustScanSubDirs: reconcile complete for path=\"/<dir>/<dir>\" (+0 -0 ~0, 6ms, 1ms CPU)"
    );
}

/// OS bookkeeping files carry no one's data and say a lot about what a walk tripped on.
#[test]
fn os_junk_names_ship_verbatim() {
    for junk in [
        ".DS_Store",
        "Thumbs.db",
        "desktop.ini",
        ".localized",
        ".Spotlight-V100",
        ".Trashes",
        ".fseventsd",
    ] {
        assert_eq!(
            r(&format!("skipped /Users/me/Photos/{junk}")),
            format!("skipped $HOME/<dir>/{junk}"),
            "{junk}"
        );
        assert_eq!(context().redact_name(junk, false), junk, "{junk}");
    }
    assert_eq!(context().redact_name("Icon\r", false), "Icon\r");
    // Inside a path, too: the walk went through it.
    assert_eq!(
        r("skipped /Volumes/Stick/.Trashes/501/x.jpg"),
        "skipped /Volumes/<volume>/.Trashes/<dir>/<file>.jpg"
    );
    // Only the exact name: anything around it is the user's own naming.
    assert_eq!(
        r("skipped /Users/me/Photos/My.DS_Store.bak"),
        "skipped $HOME/<dir>/<file>.bak"
    );
    // An app-bundle-shaped folder keeps the legacy extension guess: no producer said it's a dir.
    assert_eq!(
        r("/Users/me/Library/Application Support/com.example.app"),
        "$HOME/Library/Application Support/<file>.app"
    );
}

/// A failed move's two WARN lines (`move` and `write_operations`) printed its
/// `WriteOperationError::PermissionDenied` through the derived `Debug`, unmarked. The fallback
/// ends a prose path at `, ` after its last separator, so a file name holding a comma-space
/// shipped its tail in clear (cmdr-reports#103, ERR-KU8UY). Both lines log `log_detail()`.
#[test]
fn a_failed_move_ships_no_part_of_a_file_name_holding_a_comma() {
    use cmdr_fs::error_detail::ErrorDetail;
    use cmdr_operations::WriteOperationError;

    // A home path, so the constructor's mounted-share probe has nothing to `statfs`.
    let path = "/Users/me/Shared/Contracts/2026/Lease, Kovács Anna.pdf";
    // The server refused the staged temp beside it, which `message` names
    // (`map_volume_error`, as `ErrorDetail::path`).
    let temp = "/Users/me/Shared/Contracts/2026/.cmdr-tmp-1a2b3c-Lease, Kovács Anna.pdf";
    let error = WriteOperationError::permission_denied(path.to_string(), ErrorDetail::path(temp), None, None, None);
    let private = ["Lease", "Kovács", "Anna", "Contracts"];
    let lines = [
        format!("WARN move: move operation op-1 failed: detail={:?}", error.log_detail()),
        format!(
            "WARN write_operations: op op-1 (Move) failed: detail={:?}",
            error.log_detail()
        ),
        // A `MutationError` that wraps it keeps it typed, so its log text marks the path too.
        format!(
            "WARN rename: {}",
            cmdr_fs::log_error::LogError(&cmdr_operations::MutationError::ArchiveEditCouldntStart {
                detail: ErrorDetail::of(error.clone()),
            })
        ),
    ];
    for line in &lines {
        assert_ships_none(line, &private);
    }
    // The shape it replaced: the derived `Debug` reaches the fallback unmarked, which reads the
    // path only up to the `, `. Pinned so the site fix stays the fix.
    let derived = format!("WARN move: move operation op-1 failed: {error:?}");
    assert!(r(&derived).contains("Anna"), "{}", r(&derived));
}

#[test]
fn leak_shapes_redact_idempotently() {
    let context = context();
    for input in [
        "list_directory_core: path=/Users/me/Pictures/Astro/2026/Summer (Lake House) 2026, entries=5",
        "copying /Users/me/Pictures/A/B/Summer (Lake House) 2026 -> /Volumes/Backup/A/B/Summer (Lake House) 2026",
        "Copy error: detail=\"Path: /Volumes/Backup/A/B/Summer (Lake House) Trip/2026-08-24/Sub/.DS_Store; Error type: io_error\"",
        "resolve failed for name: \"Alex's MacBook Pro (2)._companion-link._tcp.local.\"",
        "copy_between_volumes: source_volume=Macintosh HD, dest_volume=Family Photos, 2 sources",
    ] {
        let once = context.redact_line(input).into_owned();
        assert_eq!(context.redact_line(&once), once, "input: {input:?}");
        let bare = r(input);
        assert_eq!(r(&bare), bare, "input: {input:?}");
    }
}
