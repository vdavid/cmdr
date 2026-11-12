//! The span pass: values a producer marked are hashed exactly, by their kind, at any escape depth,
//! and broken or hostile markers fail closed. Lines are built by hand the way a wrapper writes them
//! (`DETAILS.md` § "Marked spans"); the producer API comes later and feeds these same shapes.

use super::*;
use cmdr_fs::log_private::{CLOSER, LogKind, READER_SCRATCH, RESERVED};

const TEST_PROCESS_SECRET: [u8; 32] = [0x4b; 32];
pub(super) const HEAD: &str = "2026-10-19T10:00:00.000+02:00 INFO  ·listing  ";

pub(super) fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-SPANS")
}

pub(super) fn r(s: &str) -> String {
    redact_line(s).into_owned()
}

/// The salted output with every hash dropped, so it compares against the unsalted shape.
pub(super) fn shape(s: &str) -> String {
    let out = context().redact_line(s).into_owned();
    Regex::new(r"<([a-z0-9-]+):[0-9a-f]{12}>")
        .expect("valid token regex")
        .replace_all(&out, "<$1>")
        .into_owned()
}

/// Every `<kind:hash>` hash in a salted line, in order.
pub(super) fn hashes(line: &str) -> Vec<String> {
    Regex::new(r"<[a-z0-9-]+:([0-9a-f]{12})>")
        .expect("valid token regex")
        .captures_iter(line)
        .map(|caps| caps[1].to_string())
        .collect()
}

/// Both APIs give `expected` (salted modulo hashes), and a second pass changes nothing.
fn assert_redacts_to(input: &str, expected: &str) {
    assert_eq!(r(input), expected, "unsalted, input: {input:?}");
    assert_eq!(shape(input), expected, "salted, input: {input:?}");
    let salted = context().redact_line(input).into_owned();
    assert_eq!(context().redact_line(&salted), salted, "idempotent");
    assert_eq!(r(&r(input)), r(input), "idempotent unsalted");
}

/// No word of `private` and no reserved char (raw or escaped) reaches either API's output.
pub(super) fn assert_ships_none(input: &str, private: &[&str]) {
    for out in [r(input), context().redact_line(input).into_owned()] {
        for word in private {
            assert!(!out.contains(word), "{word:?} shipped: {out}");
        }
        assert!(!out.chars().any(|c| RESERVED.contains(&c)), "a marker shipped: {out:?}");
        assert!(!out.contains("u{fd"), "an escaped marker shipped: {out:?}");
    }
}

/// What Rust's `Debug` for `str` writes between its quotes.
fn body(value: &str) -> String {
    let quoted = format!("{value:?}");
    quoted[1..quoted.len() - 1].to_string()
}

/// A span as a `{}` site writes it: raw markers around the value's `{:?}` body (depth 0).
pub(super) fn span(kind: LogKind, value: &str) -> String {
    format!("{}{}{CLOSER}", kind.opener(), body(value))
}

/// A span as a `{:?}` site writes it: the same, inside quotes.
pub(super) fn quoted(kind: LogKind, value: &str) -> String {
    format!("\"{}\"", span(kind, value))
}

/// `text` one `{:?}` deeper, as `LogDetail` writes it: its spans land one depth down.
fn debug(text: &str) -> String {
    format!("{text:?}")
}

/// What the frontend's body-only `debugQuote` writes: `"`, `\`, control, bidi-format, and
/// reserved chars escaped, everything else raw (combining marks included).
fn fe_body(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() || RESERVED.contains(&c) || ('\u{202A}'..='\u{202E}').contains(&c) => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[test]
fn a_balanced_span_gets_the_token_of_its_kind() {
    let cases = [
        (
            LogKind::FilePath,
            "/Users/me/Documents/Summer (Lake House) 2026.pdf",
            "$HOME/Documents/<file>.pdf",
        ),
        (LogKind::FilePath, "/Users/me/Photos/.DS_Store", "$HOME/<dir>/.DS_Store"),
        // The producer's kind beats the extension guess, both ways.
        (LogKind::FilePath, "/Users/me/notes/Makefile", "$HOME/<dir>/<file>"),
        (
            LogKind::DirPath,
            "/Volumes/Backup/com.example.app",
            "/Volumes/<volume>/<dir>",
        ),
        (
            LogKind::Path,
            "/srv/data/Test Person/tax.pdf",
            "/<dir>/<dir>/<dir>/<file>.pdf",
        ),
        (LogKind::Path, "Photos/Summer (Lake House)", "<dir>/<dir>"),
        (LogKind::Name, "Summer (Lake House) 2026.jpg", "<file>.jpg"),
        (LogKind::FileName, "README", "<file>"),
        (LogKind::DirName, "photos.2026.pdf", "<dir>"),
        (LogKind::Volume, "Family Photos", "<volume>"),
        (LogKind::Host, "Tester's MacBook Pro (2)", "<host>"),
        (LogKind::User, "test_user_11.person", "<user>"),
    ];
    for (kind, value, token) in cases {
        // Prose after the span survives: the producer said exactly where the value ends.
        assert_redacts_to(
            &format!("{HEAD}listed {} in 3ms, 5 entries", span(kind, value)),
            &format!("{HEAD}listed {token} in 3ms, 5 entries"),
        );
        assert_redacts_to(
            &format!("{HEAD}listed value={}, entries=5", quoted(kind, value)),
            &format!("{HEAD}listed value=\"{token}\", entries=5"),
        );
    }
}

#[test]
fn a_span_inside_a_keyed_field_keeps_its_token_and_the_field() {
    let cases = [
        (
            format!("path={}", quoted(LogKind::DirPath, "/Users/me/Trips/2026")),
            "path=\"$HOME/<dir>/<dir>\"",
        ),
        (format!("host={}", quoted(LogKind::Host, "nas-west")), "host=\"<host>\""),
        (
            format!("share={}", quoted(LogKind::Volume, "Family Photos")),
            "share=\"<volume>\"",
        ),
        (
            format!("user={}", quoted(LogKind::User, "test_user_11")),
            "user=\"<user>\"",
        ),
        (
            format!("share={}, tree_id=7", span(LogKind::Volume, "Family Photos")),
            "share=<volume>, tree_id=7",
        ),
        (
            format!("path={}, n=2", span(LogKind::FilePath, "docs/x y.pdf")),
            "path=<dir>/<file>.pdf, n=2",
        ),
    ];
    for (field, expected) in cases {
        assert_redacts_to(&format!("{HEAD}connect {field}"), &format!("{HEAD}connect {expected}"));
    }
}

/// One spelling, one token: raw markers, a `LogDetail` around them, and a Debug of that.
#[test]
fn the_same_value_hashes_the_same_at_every_depth() {
    let value = "/Users/me/Trips/Summer (Lake House) \"Trip\"\\2026 Cafe\u{301}.pdf";
    let depth0 = format!("{HEAD}open path={} failed", quoted(LogKind::FilePath, value));
    let foreign = format!("can't read {}: denied", span(LogKind::FilePath, value));
    let depth1 = format!("{HEAD}open failed detail={}", debug(&foreign));
    let depth2 = format!("{HEAD}open failed detail={}", debug(&debug(&foreign)));
    let raw = format!("{HEAD}open path={value:?} failed");

    let context = context();
    let leaf = |line: &str| {
        let out = context.redact_line(line).into_owned();
        assert!(
            !out.contains("Lake") && !out.contains("Trip") && !out.contains("Caf"),
            "{out}"
        );
        hashes(&out)
            .last()
            .cloned()
            .unwrap_or_else(|| panic!("no token in {out}"))
    };
    let expected = leaf(&raw);
    for line in [&depth0, &depth1, &depth2] {
        assert_eq!(leaf(line), expected, "{line}");
    }
}

#[test]
fn an_unbalanced_opener_redacts_to_the_end_of_the_line() {
    let opener = LogKind::FilePath.opener();
    for tail in ["", "\n", "\r\n"] {
        assert_redacts_to(
            &format!("{HEAD}copying {opener}/Users/me/Secret Dir/fi{tail}"),
            &format!("{HEAD}copying <redacted>{tail}"),
        );
    }
    assert_redacts_to(
        &format!("{HEAD}copying {opener}/Users/me/Secret Dir/fi… (12 more), done"),
        &format!("{HEAD}copying <redacted>"),
    );
    // Inside a quoted field the cut ends at the field's closing quote.
    assert_redacts_to(
        &format!("{HEAD}copying path=\"{opener}/Users/me/Secret Dir/fi\" size=5"),
        &format!("{HEAD}copying path=\"<redacted>\" size=5"),
    );
}

#[test]
fn a_stray_closer_redacts_back_to_the_message_and_on_through_the_name() {
    let line = format!("{HEAD}moved /Users/me/Secret Dir/evil{CLOSER}rest/child.pdf, 3 bytes");
    assert_ships_none(&line, &["Secret", "evil", "rest", "child"]);
    assert_redacts_to(&line, &format!("{HEAD}<redacted>, 3 bytes"));

    // Back only to the previous span: what it hashed stays.
    let line = format!(
        "{HEAD}kept {} then evil{CLOSER}rest/child.pdf: gone",
        span(LogKind::FileName, "notes.md")
    );
    assert_ships_none(&line, &["evil", "rest", "child"]);
    assert_redacts_to(&line, &format!("{HEAD}kept <file>.md<redacted>: gone"));

    // No header to keep: the whole message start goes.
    assert_redacts_to(&format!("evil{CLOSER}rest/child.pdf"), "<redacted>");
}

/// A second same-depth opener before the closer doesn't nest: the run is one value, and the
/// closer left over is stray.
#[test]
fn an_opener_inside_an_open_span_widens_it() {
    let line = format!(
        "{HEAD}x {}alpha {}bravo{CLOSER} charlie{CLOSER} delta",
        LogKind::Name.opener(),
        LogKind::Name.opener()
    );
    assert_ships_none(&line, &["alpha", "bravo", "charlie", "delta"]);
    assert_redacts_to(&line, &format!("{HEAD}x <dir><redacted>"));
}

/// Hostile names, as a wrapper writes them after escaping: none ends its span early, so the prose
/// after it survives and nothing of the name ships.
#[test]
fn a_hostile_name_never_shortens_its_span() {
    let mut hostile: Vec<String> = RESERVED.map(|c| format!("a{c}b")).collect();
    hostile.extend(
        [
            "a\\u{fdd1}b",
            "a\\\\u{fdd1}b",
            "a\"b",
            "a\nb",
            "a: b",
            "a, b",
            "a key=b",
            "a)b",
            "a -> b",
            "<dir:abcdefabcdef>",
            "<file:abc>rest",
        ]
        .map(String::from),
    );
    for name in &hostile {
        let value = format!("/Users/me/Secret Folder/{name} Tail.pdf");
        let lines = [
            format!("{HEAD}open {} done", span(LogKind::FilePath, &value)),
            format!("{HEAD}open path={} done", quoted(LogKind::FilePath, &value)),
            format!(
                "{HEAD}open detail={} done",
                debug(&format!("denied {} here", span(LogKind::FilePath, &value)))
            ),
        ];
        for line in &lines {
            assert_ships_none(line, &["Secret", "Folder", "Tail"]);
            let out = r(line);
            assert!(out.ends_with(" done"), "{name:?} cut its span short: {out}");
            assert!(out.contains("$HOME/<dir>/<file>.pdf"), "{name:?}: {out}");
        }
    }
}

/// The same hostile chars printed raw by a site nobody migrated: whatever the markers look like,
/// nothing of the path ships.
#[test]
fn a_raw_hostile_name_in_an_unmarked_path_fails_closed() {
    let mut hostile: Vec<String> = RESERVED.map(|c| format!("a{c}b")).collect();
    hostile.extend(["a\\u{fdd1}b", "a\\\\u{fdd1}b", "\\u{fdd2}a\\u{fdd1}"].map(String::from));
    for name in &hostile {
        let line = format!("{HEAD}open /Users/me/Secret Folder/{name} Tail.pdf done");
        assert_ships_none(&line, &["Secret", "Folder", "Tail"]);
        let out = r(&line);
        assert!(out.starts_with(HEAD), "the header goes nowhere: {out}");
    }
}

#[test]
fn literal_marker_text_inside_a_value_sits_one_depth_deeper_than_its_closer() {
    // A raw span holding literal `\u{fdd1}` text, and a depth-1 span holding `\\u{fdd1}`.
    let literal = "Trip\\u{fdd1}2026";
    let doubled = "Trip\\\\u{fdd1}2026";
    let in_detail = |value: &str| {
        format!(
            "{HEAD}a detail={} b",
            debug(&format!("x {} y", span(LogKind::DirName, value)))
        )
    };
    let lines = [
        format!("{HEAD}a {} b", span(LogKind::DirName, literal)),
        in_detail(doubled),
        in_detail(literal),
    ];
    for line in &lines {
        assert_ships_none(line, &["Trip"]);
        assert!(r(line).ends_with(" b"), "{}", r(line));
    }
    let context = context();
    let token = |line: &str| hashes(&context.redact_line(line)).first().cloned();
    assert_eq!(
        token(&lines[0]),
        token(&lines[2]),
        "one literal, one token at both depths"
    );
}

/// A backslash at the end of a value makes the run before the closer longer (5 at depth 1, 10 at
/// depth 2), and foreign text ending in `\` makes the run before an opener 3. Depth comes from the
/// run's trailing zeros, so each still pairs where it should and hashes like the raw spelling.
#[test]
fn a_backslash_next_to_a_marker_never_shifts_its_depth() {
    let value = "Summer Trip\\";
    let foreign = format!("C:\\{}", span(LogKind::DirName, value));
    let depth1 = format!("{HEAD}a detail={} b", debug(&foreign));
    let depth2 = format!("{HEAD}a detail={} b", debug(&debug(&foreign)));
    assert!(
        depth1.contains(&format!("\\\\\\\\\\u{{{:x}}}", CLOSER as u32)),
        "run 5: {depth1}"
    );
    assert!(
        depth1.contains(&format!("C:\\\\\\u{{{:x}}}", LogKind::DirName.opener() as u32)),
        "run 3"
    );

    let context = context();
    let token = |line: &str| {
        let out = context.redact_line(line).into_owned();
        assert!(!out.contains("Summer") && !out.contains("Trip"), "{out}");
        assert!(out.ends_with(" b"), "{out}");
        hashes(&out)
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("no token: {out}"))
    };
    let expected = token(&format!("{HEAD}a {} b", span(LogKind::DirName, value)));
    assert_eq!(token(&depth1), expected);
    assert_eq!(token(&depth2), expected);
}

/// The frontend's spellings, built by hand from the bridge's contract: a leaf placeholder at depth
/// 0 and a span inside a `debugQuote`d detail at depth 1 (also as a JSON identity pair). Its body
/// leaves an NFD accent raw where Rust escapes it; the value unescapes the same.
#[test]
fn frontend_spellings_hash_like_rust_ones() {
    let value = "Summer \"Lake\" \\ Cafe\u{301}";
    let opener = LogKind::Name.opener();
    let fe_span = format!("{opener}{}{CLOSER}", fe_body(value));
    let fe_leaf = format!("{HEAD}FE: open name=\"{fe_span}\" failed");
    let fe_detail = format!(
        "{HEAD}FE: open failed detail=\"{}\"",
        fe_body(&format!("can't open {fe_span}"))
    );
    let fe_json = format!(
        "{HEAD}FE: open failed detail=\"{}\"",
        fe_body(&format!("{{\"name\":\"{fe_span}\",\"code\":5}}"))
    );
    let rust = format!("{HEAD}open name={} failed", quoted(LogKind::Name, value));

    let context = context();
    let token = |line: &str| {
        let out = context.redact_line(line).into_owned();
        assert!(!out.contains("Lake") && !out.contains("Caf"), "{out}");
        hashes(&out)
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("no token: {out}"))
    };
    let expected = token(&rust);
    for line in [&fe_leaf, &fe_detail, &fe_json] {
        assert_eq!(token(line), expected, "{line}");
    }
}

#[test]
fn an_unmarked_line_takes_the_fast_path() {
    for line in [
        "Reconciler: switched to live mode",
        "listing path=\"docs/Cafe\\u{301}.pdf\" (5 entries)",
    ] {
        assert!(markers::read_spans(line, None).is_none(), "{line}");
    }
    assert!(matches!(
        redact_line("Reconciler: switched to live mode"),
        Cow::Borrowed(_)
    ));
}

/// What the span pass costs: the golden corpus (no markers) through `redact_line` against the
/// regex pass alone, and the same corpus with one marked value per line. Prints, asserts nothing.
#[test]
#[ignore = "a timing, run by hand: cargo nextest run -p cmdr --lib --run-ignored only span_pass_cost --no-capture"]
#[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
fn span_pass_cost() {
    use std::hint::black_box;
    use std::time::Instant;
    let corpus: Vec<&str> = include_str!("fixtures/log-corpus.txt").lines().collect();
    let marked: Vec<String> = corpus
        .iter()
        .map(|line| {
            format!(
                "{line} in {}",
                span(LogKind::FilePath, "/Users/me/Trips/Summer (Lake House) 2026.pdf")
            )
        })
        .collect();
    let rounds = 10;
    let time = |label: &str, run: &dyn Fn()| {
        run();
        let start = Instant::now();
        for _ in 0..rounds {
            run();
        }
        let per_line = start.elapsed() / (rounds * corpus.len() as u32);
        println!("{label}: {per_line:?} per line");
    };
    time("unmarked, regex pass alone", &|| {
        for line in &corpus {
            black_box(scan(line, None, &Spans::NONE));
        }
    });
    time("unmarked, redact_line", &|| {
        for line in &corpus {
            black_box(redact_line(line));
        }
    });
    time("marked, redact_line", &|| {
        for line in &marked {
            black_box(redact_line(line));
        }
    });
}

/// A balanced pair inside a path some unmigrated site printed whole, and a wrapped dir glued to an
/// unwrapped name: the path reads as one path across the span, so nothing on either side ships.
#[test]
fn a_path_keeps_its_context_across_a_span() {
    let (open, close) = (LogKind::Path.opener(), CLOSER);
    for line in [
        format!("{HEAD}scan /Users/me/Photos/My {open}a{close} rest/child dir/x.pdf"),
        format!("{HEAD}scan /Users/me/{open}a{close}Real Name/child/Secret.pdf"),
        format!(
            "{HEAD}scan {}/Secret Child/x.pdf done",
            span(LogKind::DirPath, "/Volumes/Backup/Real Name")
        ),
        format!("{HEAD}scan {}rest/child.pdf", span(LogKind::DirName, "Real Name")),
        format!("{HEAD}copy smb://nas/share/My {open}a{close} rest/x.pdf"),
        format!("{HEAD}tree: connected share=Fam {open}a{close} ily Photos, tree_id=7"),
        format!(
            "{HEAD}mdns WARN name: \"Tester{}s Mac{close}Book Pro._companion-link._tcp.local.\"",
            LogKind::FileName.opener()
        ),
    ] {
        assert_ships_none(
            &line,
            &[
                "Photos/", "rest", "child", "Real", "Name", "Secret", "Child", "ily", "Book",
            ],
        );
    }
}

/// A cut opener inside `detail="…"` ends at the field's closing quote, so the field stays one
/// field: still capped, still scrubbed of the line's keyed identities.
#[test]
fn a_cut_span_inside_a_detail_keeps_the_field_whole() {
    let line = format!(
        "{HEAD}tree connect failed server=\"nas-west\" detail={} code=5",
        debug(&format!(
            "nas-west refused {}/Volumes/Family Photos/Secr",
            LogKind::FilePath.opener()
        ))
    );
    assert_ships_none(&line, &["nas-west", "Family", "Secr"]);
    let out = r(&line);
    assert!(out.ends_with(" code=5"), "{out}");
    assert!(out.contains("detail=\"<host> refused <redacted>\""), "{out}");
}

#[test]
fn a_span_leaf_repeated_bare_on_its_line_is_scrubbed() {
    let line = format!(
        "{HEAD}the Trash refused {}: “Tax Return 2026.pdf” is locked",
        span(LogKind::FilePath, "/Users/me/Desktop/Tax Return 2026.pdf")
    );
    assert_ships_none(&line, &["Tax", "Return"]);
    assert_redacts_to(
        &line,
        &format!("{HEAD}the Trash refused $HOME/Desktop/<file>.pdf: “<file>.pdf” is locked"),
    );
    // A keyed identity span is scrubbed from external text, like a `host=` field.
    let line = format!(
        "{HEAD}connect host={} detail={}",
        quoted(LogKind::Host, "nas-west"),
        debug("nas-west refused the login")
    );
    assert_ships_none(&line, &["nas-west"]);
}

/// Any mix of markers, escapes, quotes, and path text: the pass never panics on a slice, and
/// nothing reserved survives it, raw or escaped.
#[test]
fn arbitrary_marker_soup_never_panics_or_ships_a_marker() {
    let opener = LogKind::FilePath.opener().to_string();
    let parts = [
        opener.as_str(),
        "\u{FDD1}",
        "\u{FDEF}",
        "\\",
        "\\u{fdd1}",
        "\\u{FDD2}",
        "\"",
        "path=\"",
        " ",
        "/Users/me/",
        "Secret",
        ", ",
        ": ",
        "\n",
    ];
    // A fixed linear congruential sequence, so a failure reproduces.
    let mut state: u64 = 0x2026_1019;
    for _ in 0..1500 {
        let mut line = HEAD.to_string();
        for _ in 0..8 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            line.push_str(parts[(state >> 33) as usize % parts.len()]);
        }
        for out in [r(&line), context().redact_line(&line).into_owned()] {
            assert!(!out.chars().any(|c| RESERVED.contains(&c)), "{line:?} -> {out:?}");
            assert!(!out.to_lowercase().contains("u{fd"), "{line:?} -> {out:?}");
        }
        let shown = display_line(&line);
        assert!(!shown.chars().any(|c| RESERVED.contains(&c)), "{line:?} -> {shown:?}");
        assert!(!shown.to_lowercase().contains("u{fd"), "{line:?} -> {shown:?}");
    }
}

/// The reader's own scratch chars arriving in a typed value are a name like any other.
#[test]
fn a_typed_value_never_passes_the_readers_scratch_chars_through() {
    let scratch: String = [*READER_SCRATCH.end(), *READER_SCRATCH.start()].iter().collect();
    let context = context();
    for out in [
        context.redact_name(&scratch, true),
        context.redact_path(&format!("/Users/me/{scratch}/x.pdf")),
        context.redact_volume_name(&scratch),
    ] {
        assert!(!out.chars().any(|c| RESERVED.contains(&c)), "{out:?}");
    }
}

/// ❗ A same-named twin's row path carries a NUL pick (`cmdr_fs::twin_path`): the
/// log a person reads shows its plain path, never the NUL and the handle.
#[test]
fn a_twin_row_path_reads_as_its_plain_path_in_the_shown_log() {
    let twin = format!("/DCIM/{}/IMG_1.JPG", cmdr_fs::twin_path::pick_segment(4242));
    let line = format!("deleting {}", span(LogKind::FilePath, &twin));

    assert_eq!(display_line(&line), "deleting /DCIM/IMG_1.JPG");
}
