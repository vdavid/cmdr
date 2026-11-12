//! Names in ordinary prose: an absolute path under any prefix, and a path's leaf repeated bare
//! elsewhere on its line.

use super::*;

const TEST_PROCESS_SECRET: [u8; 32] = [0x7c; 32];

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-PROSE")
}

fn r(s: &str) -> String {
    redact_line(s).into_owned()
}

/// Hundreds of sites log `{}` of `path.display()`, and a server, an archive, or a phone names
/// paths under prefixes no dedicated branch knows (`/srv`, `/mnt`, `/DCIM`, `/sdcard`).
#[test]
fn an_absolute_path_under_any_prefix_is_redacted() {
    let cases = [
        (
            "Cover: can't walk /srv/clients/Acme Corp/plans.pdf: permission denied",
            "Cover: can't walk /<dir>/<dir>/<dir>/<file>.pdf: permission denied",
        ),
        (
            "SftpVolume::create_directory_all: /data/Secret Project",
            "SftpVolume::create_directory_all: /<dir>/<dir>",
        ),
        (
            "skipping too-small image '/mnt/backup/Test Person/tax.pdf' (12x12)",
            "skipping too-small image '/mnt/<dir>/<dir>/<file>.pdf' (12x12)",
        ),
        (
            "Upload failed for /DCIM/Camera/IMG_0042.HEIC, on the phone: busy",
            "Upload failed for /<dir>/<dir>/<file>.HEIC, on the phone: busy",
        ),
        (
            "Installing update into bundle: /Applications/Cmdr.app",
            "Installing update into bundle: /Applications/<file>.app",
        ),
        // A `(`-led word belongs to the name: `(Full)` could be the end of `Secret Corp (Full)`.
        (
            "Phases: covering /Library/Secret Corp (Full)",
            "Phases: covering /Library/<dir>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// A slash inside a word, a unit, a ratio, or a lone root isn't a path.
#[test]
fn slashes_that_are_not_absolute_paths_stay() {
    for input in [
        "copied 12 MB/s over TCP/IP",
        "read and/or write 3/4 of it",
        "MustScanSubDirs: reconcile slow for / (+38 -0 ~515676, 1691s)",
        "apps/desktop/src-tauri/src/lib.rs:12:5",
        "$HOME/Documents/<file:0123456789ab>.pdf",
    ] {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
}

/// macOS repeats the file's name in its own error text, so the Trash line carried the name
/// twice: once in the path, which was redacted, and once bare, which shipped.
#[test]
fn a_path_leaf_repeated_bare_on_its_line_is_scrubbed() {
    let context = context();
    let input = "op 01a0 (Trash) failed: /Users/test_user_9/Desktop/Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg: \
                 the Trash refused it: “Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg”";
    let out = context.redact_line(input).into_owned();
    assert!(!out.contains("Screenshot") && !out.contains("01.13.03"), "{out}");
    let leaf = context.redact_name("Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg", false);
    assert_eq!(out.matches(&leaf).count(), 2, "both copies share one token: {out}");
    assert!(out.ends_with(&format!("the Trash refused it: “{leaf}”")), "{out}");
}

/// The echo comes first just as often (`couldn't open "x" at /path/x`).
#[test]
fn a_leaf_named_before_its_path_is_scrubbed_too() {
    let out = r("couldn't open \"Budget 2026.xlsx\" at /Volumes/Data/Budget 2026.xlsx, giving up");
    assert_eq!(
        out,
        "couldn't open \"<file>.xlsx\" at /Volumes/<volume>/<file>.xlsx, giving up"
    );
}

/// The log header names nobody: a folder called `2026` or `INFO` leaves the timestamp and the
/// level as they are.
#[test]
fn the_leaf_scrub_leaves_the_log_header_alone() {
    let header = "2026-10-19T10:00:00.000+02:00 INFO  ·listing  ";
    assert_eq!(
        r(&format!("{header}listing /Users/me/Trips/2026, 2026 entries")),
        format!("{header}listing $HOME/<dir>/<dir>, <dir> entries")
    );
    assert_eq!(
        r(&format!("{header}listing /srv/x/INFO, 3 entries")),
        format!("{header}listing /<dir>/<dir>/<dir>, 3 entries")
    );
}

/// Only whole-word repeats go, and only of names long enough not to be part of prose.
#[test]
fn the_leaf_scrub_leaves_other_words_alone() {
    assert_eq!(
        r("plans.pdf vs myplans.pdf in /srv/x/plans.pdf"),
        "<file>.pdf vs myplans.pdf in /<dir>/<dir>/<file>.pdf"
    );
    assert_eq!(r("ab in /srv/x/ab"), "ab in /<dir>/<dir>/<dir>");
}

/// A log target shares words with folder names; `_` and `::` glue them into one identifier.
#[test]
fn the_leaf_scrub_spares_log_targets() {
    assert_eq!(
        r("10:19:34.908 DEBUG media_index  media walk stopped in /srv/x/media"),
        "10:19:34.908 DEBUG media_index  <dir> walk stopped in /<dir>/<dir>/<dir>"
    );
    assert_eq!(
        r("10:19:34.908 DEBUG indexing::writer  writer stalled on /srv/x/writer"),
        "10:19:34.908 DEBUG indexing::writer  <dir> stalled on /<dir>/<dir>/<dir>"
    );
}

#[test]
fn prose_redaction_is_idempotent() {
    let context = context();
    for input in [
        "Cover: can't walk /srv/clients/Acme Corp/plans.pdf: permission denied",
        "op 01a0 (Trash) failed: /Users/k/Desktop/Shot 1.jpeg: the Trash refused it: “Shot 1.jpeg”",
        "Installing update into bundle: /Applications/Cmdr.app",
    ] {
        let once = context.redact_line(input).into_owned();
        assert_eq!(context.redact_line(&once), once, "input: {input:?}");
        let bare = r(input);
        assert_eq!(r(&bare), bare, "input: {input:?}");
    }
}

/// The first segment under a system root takes more words like every other segment: it used to
/// stop at the first space, and the rest of the name shipped (`/tmp/<dir> Plans/beach.jpg`).
#[test]
fn a_multi_word_first_segment_under_a_system_root_stays_whole() {
    assert_eq!(
        r("scratch at /tmp/Secret Plans/beach.jpg"),
        "scratch at /tmp/<dir>/<file>.jpg"
    );
    assert_eq!(
        r("cache in /private/Secret Plans: busy"),
        "cache in /private/<dir>: busy"
    );
}

/// `<` and `>` are legal in a macOS or Linux file name, so neither may end a path: a tail that
/// stopped at one shipped the rest of the name (`Secret <draft> Plans/beach.jpg` lost only
/// `Secret`). Unmarked prose, keyed fields, quoted and bare, and a name that starts with `<`.
#[test]
fn angle_brackets_inside_a_name_never_end_its_path() {
    let context = context();
    for input in [
        "copy failed for /Users/test/Photos/Secret <draft> Plans/beach.jpg",
        "copy failed for /Users/test/Photos/Secret<draft>Plans/beach.jpg, retrying",
        "listing /Volumes/Backup/Secret >draft< Plans/beach.jpg took 3ms",
        "listing /media/usb/Secret <draft> Plans: permission denied",
        "scratch at /tmp/Secret <draft> Plans/beach.jpg",
        "walking /srv/share/Secret <draft> Plans/beach.jpg",
        "skipping /Users/test/Photos/<draft> Secret Plans/beach.jpg",
        "skipping /srv/share/<draft> Secret Plans/beach.jpg",
        "listing path=\"/Users/test/Photos/Secret <draft> Plans/beach.jpg\" took 3ms",
        "listing path=\"Photos/Secret <draft> Plans/beach.jpg\" took 3ms",
        "tree: renamed from=Photos/Secret <draft> Plans/beach.jpg to=x.jpg",
        "copying /Users/test/Photos/Secret <draft> Plans -> /Volumes/Backup/Secret <draft> Plans",
    ] {
        for out in [r(input), context.redact_line(input).into_owned()] {
            for word in ["Secret", "draft", "Plans", "beach"] {
                assert!(!out.contains(word), "{word:?} shipped from {input:?}: {out}");
            }
        }
    }
}
