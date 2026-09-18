//! Tests for lifting macOS's own crash report into ours.
//!
//! The fixtures are trimmed real `.ips` shapes: a one-line JSON header, then the body. Field names
//! and nesting match what `ReportCrash` writes (checked against a real `Cmdr-*.ips` on macOS 26.6.2,
//! 2026-09-18); only the thread count and image list are cut down.

use super::*;

/// A report body with one faulting thread, two images, and a symbolicated frame each.
fn an_ips(body: serde_json::Value) -> String {
    let header = serde_json::json!({ "app_name": "Cmdr", "app_version": "0.45.1", "bug_type": "309" });
    format!("{header}\n{body}")
}

fn a_body() -> serde_json::Value {
    serde_json::json!({
        "exception": {
            "type": "EXC_BAD_ACCESS",
            "signal": "SIGSEGV",
            "subtype": "KERN_INVALID_ADDRESS at 0x0000000000000010",
            "codes": "0x0000000000000001, 0x0000000000000010",
        },
        "faultingThread": 1,
        "threads": [
            { "frames": [{ "imageIndex": 0, "symbol": "not_the_faulting_thread", "symbolLocation": 1 }] },
            {
                "queue": "com.apple.main-thread",
                "frames": [
                    { "imageIndex": 0, "symbol": "WebKit::RemoteLayerTreeDrawingAreaProxy::commitLayerTree", "symbolLocation": 280 },
                    { "imageIndex": 1, "symbol": "__CFRunLoopRun", "symbolLocation": 1992 },
                    { "imageIndex": 1, "imageOffset": 262144 },
                ],
            },
        ],
        "usedImages": [
            { "name": "WebKit", "path": "/System/Library/Frameworks/WebKit.framework/WebKit" },
            { "name": "CoreFoundation", "path": "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation" },
        ],
        // Everything below is deliberately NOT in the allowlist, and no test may start asserting
        // that it survives: these are the fields that identify a machine or name another app.
        "crashReporterKey": "4BB1AFE6-7CF8-18DC-23A7-9A38ED2D35A1",
        "coalitionName": "dev.warp.Warp-Stable",
        "parentProc": "zsh",
        "userID": 501,
        "procPath": "/Users/USER/*/Cmdr",
        "bootSessionUUID": "0BE7C6B3-4D80-4B8B-A0F6-1F4C4C5E6A7B",
    })
}

#[test]
fn the_exception_line_carries_the_subtype_which_is_the_whole_diagnosis() {
    // A fault at a small address is a null dereference at that struct offset. Our own report can
    // never say this, which is the reason this module exists.
    let report = parse_report(&an_ips(a_body())).expect("the fixture yields a report");

    assert_eq!(
        report.exception.as_deref(),
        Some("EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS at 0x0000000000000010")
    );
}

#[test]
fn frames_come_from_the_faulting_thread_not_thread_zero() {
    let report = parse_report(&an_ips(a_body())).expect("the fixture yields a report");

    assert_eq!(
        report.frames,
        vec![
            "WebKit WebKit::RemoteLayerTreeDrawingAreaProxy::commitLayerTree + 280",
            "CoreFoundation __CFRunLoopRun + 1992",
            "CoreFoundation + 262144",
        ]
    );
}

#[test]
fn an_unsymbolicated_frame_falls_back_to_its_image_offset() {
    // Our own binary is the image most likely to land here, and an offset plus `imageBase` is
    // still resolvable with `atos`. Dropping the frame would silently shorten the stack.
    let report = parse_report(&an_ips(a_body())).expect("the fixture yields a report");

    assert_eq!(report.frames[2], "CoreFoundation + 262144");
}

#[test]
fn nothing_outside_the_allowlist_reaches_the_extract() {
    // Pre-fix, shipping the report's JSON wholesale would have carried every one of these.
    let report = parse_report(&an_ips(a_body())).expect("the fixture yields a report");
    let rendered = format!("{:?}", report);

    for leaked in [
        "4BB1AFE6",                // crashReporterKey, stable per machine
        "Warp",                    // coalitionName, names whatever launched Cmdr
        "zsh",                     // parentProc
        "/Users/",                 // procPath
        "0BE7C6B3",                // bootSessionUUID
        "not_the_faulting_thread", // a frame from a thread we didn't ask for
    ] {
        assert!(!rendered.contains(leaked), "{leaked} must not survive the extract");
    }
}

#[test]
fn no_faulting_thread_means_no_frames_rather_than_a_guess() {
    // Thread 0 would be a guess, and a wrong stack costs more than a missing one.
    let mut body = a_body();
    body.as_object_mut().expect("object").remove("faultingThread");

    let report = parse_report(&an_ips(body)).expect("the exception alone still yields a report");

    assert!(report.frames.is_empty());
    assert!(report.exception.is_some());
}

#[test]
fn a_report_with_neither_exception_nor_frames_yields_nothing() {
    let body = serde_json::json!({ "crashReporterKey": "4BB1AFE6" });

    assert_eq!(parse_report(&an_ips(body)), None);
}

#[test]
fn a_partial_exception_block_renders_what_it_has() {
    let cases = [
        (
            serde_json::json!({ "type": "EXC_CRASH", "signal": "SIGABRT" }),
            "EXC_CRASH (SIGABRT)",
        ),
        (serde_json::json!({ "type": "EXC_BAD_ACCESS" }), "EXC_BAD_ACCESS"),
        (
            serde_json::json!({ "subtype": "KERN_PROTECTION_FAILURE at 0x8" }),
            "KERN_PROTECTION_FAILURE at 0x8",
        ),
    ];

    for (exception, expected) in cases {
        assert_eq!(render_exception(Some(&exception)).as_deref(), Some(expected));
    }
    assert_eq!(render_exception(Some(&serde_json::json!({}))), None);
    assert_eq!(render_exception(None), None);
}

#[test]
fn the_header_line_is_not_parsed_as_the_body() {
    // The file is two JSON documents. Parsing the header would find no `exception` and no threads.
    let text = an_ips(a_body());
    assert!(text.starts_with('{'), "the fixture keeps the real two-document shape");

    assert!(parse_report(&text).is_some());
}

#[test]
fn a_body_that_is_not_json_is_ignored_rather_than_fatal() {
    assert_eq!(parse_report("header\nnot json at all"), None);
    assert_eq!(parse_report("no newline so no body"), None);
}

#[test]
fn a_very_deep_stack_is_capped() {
    let frames: Vec<serde_json::Value> = (0..MAX_FRAMES + 20)
        .map(|i| serde_json::json!({ "imageIndex": 0, "symbol": format!("f{i}"), "symbolLocation": 0 }))
        .collect();
    let body = serde_json::json!({
        "faultingThread": 0,
        "threads": [{ "frames": frames }],
        "usedImages": [{ "name": "Cmdr" }],
    });

    let report = parse_report(&an_ips(body)).expect("the fixture yields a report");

    assert_eq!(report.frames.len(), MAX_FRAMES);
}

#[test]
fn a_giant_cpp_symbol_is_capped_by_chars_not_bytes() {
    // A byte-index cut inside a multi-byte character panics, and this code runs on the
    // crash-reporting path. The template-expansion symbols WebKit produces are the real case.
    let symbol = "ẞ".repeat(MAX_FRAME_CHARS * 2);
    let body = serde_json::json!({
        "faultingThread": 0,
        "threads": [{ "frames": [{ "imageIndex": 0, "symbol": symbol, "symbolLocation": 0 }] }],
        "usedImages": [{ "name": "WebKit" }],
    });

    let report = parse_report(&an_ips(body)).expect("the fixture yields a report");

    assert_eq!(report.frames[0].chars().count(), MAX_FRAME_CHARS);
}

#[test]
fn a_path_inside_a_symbol_is_redacted() {
    // macOS already collapses home paths in a third-party report, so this is insurance. It still
    // has to work: the redactor is the only thing between a surprising field and the network.
    let body = serde_json::json!({
        "faultingThread": 0,
        "threads": [{
            "frames": [{ "imageIndex": 0, "symbol": "opened /Users/jane/Secret/notes.txt", "symbolLocation": 0 }],
        }],
        "usedImages": [{ "name": "Cmdr" }],
    });

    let report = parse_report(&an_ips(body)).expect("the fixture yields a report");

    assert!(!report.frames[0].contains("jane"), "got {}", report.frames[0]);
    assert!(!report.frames[0].contains("Secret"), "got {}", report.frames[0]);
}

mod finding_the_file {
    use super::*;
    use std::time::Duration;

    fn write_report(dir: &Path, name: &str, modified: SystemTime) {
        let path = dir.join(name);
        std::fs::write(&path, an_ips(a_body())).expect("write fixture");
        let file = std::fs::File::options().write(true).open(&path).expect("open fixture");
        file.set_modified(modified).expect("set mtime");
    }

    #[test]
    fn the_report_closest_to_the_crash_wins() {
        // Someone who crashed twice in a minute has two reports; the newest is not always ours.
        let dir = tempfile::tempdir().expect("temp dir");
        let crash_time = SystemTime::now();
        write_report(dir.path(), "Cmdr-far.ips", crash_time + Duration::from_secs(90));
        write_report(dir.path(), "Cmdr-near.ips", crash_time + Duration::from_secs(3));

        let found = find_report(dir.path(), "Cmdr", crash_time).expect("a report matches");

        assert!(found.ends_with("Cmdr-near.ips"));
    }

    #[test]
    fn a_report_outside_the_window_is_not_ours() {
        let dir = tempfile::tempdir().expect("temp dir");
        let crash_time = SystemTime::now();
        write_report(
            dir.path(),
            "Cmdr-old.ips",
            crash_time - MATCH_WINDOW - Duration::from_secs(60),
        );

        assert_eq!(find_report(dir.path(), "Cmdr", crash_time), None);
    }

    #[test]
    fn a_helper_processs_report_is_not_ours() {
        // macOS files `cmdr_lib-<hash>-….ips` for helper processes; that stack is a different crash.
        let dir = tempfile::tempdir().expect("temp dir");
        let crash_time = SystemTime::now();
        write_report(dir.path(), "cmdr_lib-c7261f45-2026.ips", crash_time);
        write_report(dir.path(), "OtherApp-2026.ips", crash_time);
        write_report(dir.path(), "Cmdr-2026.txt", crash_time);

        assert_eq!(find_report(dir.path(), "Cmdr", crash_time), None);
    }

    #[test]
    fn an_unreadable_directory_is_a_normal_miss() {
        let dir = tempfile::tempdir().expect("temp dir");

        assert_eq!(find_report(&dir.path().join("nope"), "Cmdr", SystemTime::now()), None);
    }
}
