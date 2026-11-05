//! The viewer commands end to end over the demo's volume: the real viewer core, real
//! stored bytes, and the Tauri-shaped JSON in and out. Natively the bridge blocks on a
//! tokio runtime, so these call the viewer from a plain thread, as the app's commands do.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::testing::wait_until;

use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::tree::DemoTree;
use crate::viewer::{DemoViewer, MEDIA_TOKEN_DROPPED, OPEN_URL, VIEWER_COMMANDS, VIEWER_MEDIA_LOAD};

/// Every event, with the window it was sent to (`None` for a broadcast).
#[derive(Default)]
struct Collector(Mutex<Vec<(Option<String>, String, Value)>>);

impl EventSink for Collector {
    fn emit(&self, event: &str, payload: Value) {
        self.push(None, event, payload);
    }

    fn emit_to(&self, window_label: &str, event: &str, payload: Value) {
        self.push(Some(window_label.to_string()), event, payload);
    }
}

impl Collector {
    fn push(&self, target: Option<String>, event: &str, payload: Value) {
        self.0
            .lock()
            .expect("collector lock")
            .push((target, event.to_string(), payload));
    }

    fn named(&self, event: &str) -> Vec<(Option<String>, Value)> {
        let events = self.0.lock().expect("collector lock");
        events
            .iter()
            .filter(|(_, name, _)| name == event)
            .map(|(target, _, payload)| (target.clone(), payload.clone()))
            .collect()
    }
}

/// A one-pixel PNG's signature and header: enough to classify as an image.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n";
const NOTES: &str = "first line\nsecond line has a needle\nthird line\n";
const README: &str = "# Hello\n\nSee [the notes](notes.txt), ![a dot](dot.png), and [the site](https://getcmdr.com).\n";

/// The test's runtime, kept alive beside the demo it built.
struct Fixture {
    demo: Demo,
    events: Arc<Collector>,
    runtime: tokio::runtime::Runtime,
}

impl Fixture {
    fn viewer(&self) -> &DemoViewer {
        self.demo.viewer()
    }

    fn call(&self, command: &str, args: Value) -> Result<Value, Value> {
        self.viewer().invoke(command, args)
    }

    fn open(&self, path: &str, window: &str) -> Value {
        self.call(
            "viewer_open",
            json!({ "path": path, "volumeId": "root", "windowLabel": window, "rows": "raw" }),
        )
        .unwrap_or_else(|e| panic!("opening {path}: {e}"))
    }
}

fn fixture() -> Fixture {
    let mut tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/docs", "kind": "dir" },
            { "path": "/docs/sub", "kind": "dir" },
            { "path": "/out", "kind": "dir" },
            { "path": "/docs/notes.txt", "kind": "file", "size": 1 },
            { "path": "/docs/README.md", "kind": "file" },
            { "path": "/docs/dot.png", "kind": "file" },
            { "path": "/docs/paper.pdf", "kind": "file" },
            { "path": "/docs/big.bin", "kind": "file", "size": 4096 },
        ],
    }))
    .expect("tree parses");
    tree.contents = HashMap::from([
        ("/docs/notes.txt".to_string(), NOTES.as_bytes().to_vec()),
        ("/docs/README.md".to_string(), README.as_bytes().to_vec()),
        ("/docs/dot.png".to_string(), PNG.to_vec()),
        ("/docs/paper.pdf".to_string(), PDF.to_vec()),
    ]);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let events = Arc::new(Collector::default());
    let demo = {
        let _entered = runtime.enter();
        Demo::new(tree, Arc::clone(&events) as _).expect("the demo builds")
    };
    Fixture { demo, events, runtime }
}

#[test]
fn the_engine_answers_every_viewer_command() {
    for command in VIEWER_COMMANDS {
        assert!(Demo::handles(command), "{command}");
        assert!(Demo::commands().any(|c| c == *command), "{command}");
    }
}

#[test]
fn a_text_file_opens_reads_searches_and_closes() {
    let f = fixture();
    let opened = f.open("/docs/notes.txt", "viewer-1");
    assert_eq!(opened["kind"], "text");
    assert_eq!(opened["totalBytes"], NOTES.len());
    assert_eq!(opened["isIndexing"], false);
    let session = opened["sessionId"].as_str().expect("a session id").to_string();

    let lines = f
        .call(
            "viewer_get_lines",
            json!({ "sessionId": session, "targetType": "row", "targetValue": 1, "count": 2 }),
        )
        .expect("lines");
    assert_eq!(lines["rows"][0]["text"], "second line has a needle");

    // The browser runs a search inline, so the first poll already has the answer.
    f.call(
        "viewer_search_start",
        json!({ "sessionId": session, "query": "needle", "mode": { "useRegex": false, "caseSensitive": false } }),
    )
    .expect("search starts");
    // Natively the search runs on a thread of its own; the smoke pins the browser's inline
    // answer on the first poll.
    let poll = || {
        f.call("viewer_search_poll", json!({ "sessionId": session, "sinceIndex": 0 }))
            .expect("poll")
    };
    wait_until(Duration::from_secs(5), "the search finishes", || {
        poll()["status"]["status"] == "done"
    });
    let poll = poll();
    assert_eq!(poll["newMatches"][0]["row"], 1);
    assert_eq!(
        f.call(
            "viewer_search_start",
            json!({ "sessionId": session, "query": "", "mode": { "useRegex": false, "caseSensitive": false } })
        ),
        Err(Value::String("Search query cannot be empty".to_string()))
    );

    let bytes = f
        .call(
            "viewer_get_bytes",
            json!({ "sessionId": session, "offset": 0, "count": 5 }),
        )
        .expect("bytes");
    assert_eq!(bytes, json!(b"first".to_vec()));

    f.call("viewer_get_encoding_options", json!({ "sessionId": session }))
        .expect("encoding options");
    f.call(
        "viewer_set_encoding",
        json!({ "sessionId": session, "encoding": "windows1252" }),
    )
    .expect("an encoding switch");

    f.call("viewer_close", json!({ "sessionId": session })).expect("close");
    let gone = f.call("viewer_get_status", json!({ "sessionId": session }));
    assert!(matches!(gone, Err(Value::String(_))), "{gone:?}");
}

impl Fixture {
    /// Runs one write command to its `write-settled`, answering how it ended.
    fn write(&self, command: &str, args: Value) -> String {
        let started = self
            .runtime
            .block_on(self.demo.invoke(command, args))
            .unwrap_or_else(|e| panic!("{command} refused: {e}"));
        let id = started["operationId"].clone();
        wait_until(Duration::from_secs(10), "the write settles", || {
            self.events
                .named("write-settled")
                .iter()
                .any(|(_, p)| p["operationId"] == id)
        });
        ["write-complete", "write-cancelled", "write-error"]
            .into_iter()
            .find(|name| self.events.named(name).iter().any(|(_, p)| p["operationId"] == id))
            .expect("a terminal event")
            .to_string()
    }

    /// The first rows' text of `path`, opened in a viewer of its own.
    fn first_rows(&self, path: &str) -> Vec<String> {
        let opened = self.open(path, "viewer-w");
        assert_eq!(opened["totalBytes"], NOTES.len(), "{path}");
        let session = opened["sessionId"].as_str().expect("a session id").to_string();
        let lines = self
            .call(
                "viewer_get_lines",
                json!({ "sessionId": session, "targetType": "row", "targetValue": 0, "count": 3 }),
            )
            .expect("lines");
        self.call("viewer_close", json!({ "sessionId": session }))
            .expect("close");
        lines["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|r| r["text"].as_str().expect("text").to_string())
            .collect()
    }
}

/// What a write lands, the viewer reads: the copy, its rename, and its move all show the
/// source's text, because the viewer reads the volume the write engine wrote to.
#[test]
fn a_copied_renamed_and_moved_file_views_with_its_content() {
    let f = fixture();
    let config = json!({ "progressIntervalMs": 50, "conflictResolution": "stop", "maxConflictsToShow": 100 });
    let expected = ["first line", "second line has a needle", "third line"];

    let copied = f.write(
        "copy_between_volumes",
        json!({
            "sourceVolumeId": "root", "sourcePaths": ["/docs/notes.txt"], "destVolumeId": "root",
            "destPath": "/out", "config": config, "initiator": "user",
        }),
    );
    assert_eq!(copied, "write-complete");
    assert_eq!(f.first_rows("/out/notes.txt"), expected);

    let renamed = f
        .runtime
        .block_on(f.demo.invoke(
            "rename_file",
            json!({ "from": "/out/notes.txt", "to": "/out/renamed.txt", "force": false, "volumeId": "root", "initiator": "user" }),
        ))
        .expect("the rename");
    assert_eq!(renamed["type"], "done", "{renamed}");
    assert_eq!(f.first_rows("/out/renamed.txt"), expected);

    let moved = f.write(
        "move_files",
        json!({ "sources": ["/out/renamed.txt"], "destination": "/docs/sub", "config": config, "initiator": "user" }),
    );
    assert_eq!(moved, "write-complete");
    assert_eq!(f.first_rows("/docs/sub/renamed.txt"), expected);
}

#[test]
fn a_file_with_generated_bytes_opens_as_binary_text() {
    let f = fixture();
    let opened = f.open("/docs/big.bin", "viewer-1");
    assert_eq!(opened["totalBytes"], 4096);
}

#[test]
fn markdown_renders_follows_a_relative_link_and_serves_its_image() {
    let f = fixture();
    let opened = f.open("/docs/README.md", "viewer-md");
    assert_eq!(opened["kind"], "markdown");
    let session = opened["sessionId"].as_str().expect("a session id").to_string();

    let rendered = f
        .call("viewer_render_markdown", json!({ "sessionId": session }))
        .expect("a render");
    let html = rendered["html"].as_str().expect("the document's HTML");
    let token = html
        .split("data-md-image=\"local:")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("an image token in {html}"))
        .to_string();
    let body = f.viewer().media_body(&token, "viewer-md").expect("the image's bytes");
    assert_eq!(body.mime, "image/png");
    assert_eq!(body.bytes, PNG);

    let link = f
        .call(
            "viewer_follow_markdown_link",
            json!({ "sessionId": session, "href": "notes.txt" }),
        )
        .expect("a link");
    assert_eq!(link["kind"], "file");
    assert_eq!(link["path"], "/docs/notes.txt");

    f.call(
        "viewer_follow_markdown_link",
        json!({ "sessionId": session, "href": "https://getcmdr.com" }),
    )
    .expect("an external link");
    assert_eq!(
        f.events.named(OPEN_URL),
        vec![(None, json!({ "url": "https://getcmdr.com/" }))]
    );
}

#[test]
fn an_image_and_a_pdf_serve_their_bytes_and_tell_only_their_window() {
    let f = fixture();
    for (path, kind, bytes, window) in [
        ("/docs/dot.png", "image", PNG, "viewer-img"),
        ("/docs/paper.pdf", "pdf", PDF, "viewer-pdf"),
    ] {
        let opened = f.open(path, window);
        assert_eq!(opened["kind"], kind, "{path}");
        assert_eq!(opened["mediaFromVolume"], true, "{path}");
        let token = opened["mediaToken"].as_str().expect("a token").to_string();
        assert_eq!(f.viewer().media_body(&token, window).expect("bytes").bytes, bytes);
        let loads = f.events.named(VIEWER_MEDIA_LOAD);
        let (target, payload) = loads.last().expect("a media load");
        assert_eq!(target.as_deref(), Some(window));
        assert_eq!(payload["token"], token.as_str());
        assert_eq!(payload["stage"], "done");
    }
    assert!(f.viewer().media_body("not-a-token", "viewer-img").is_none());
}

#[test]
fn tokens_differ_and_each_drop_reaches_the_page_when_its_window_closes() {
    let f = fixture();
    let first = f.open("/docs/dot.png", "viewer-a")["mediaToken"].clone();
    let second = f.open("/docs/paper.pdf", "viewer-b")["mediaToken"].clone();
    assert_ne!(first, second);
    assert_eq!(first.as_str().map(str::len), Some(32));

    f.viewer().close_window("viewer-a");
    assert_eq!(
        f.events.named(MEDIA_TOKEN_DROPPED),
        vec![(None, json!({ "token": first }))]
    );
    let token = first.as_str().expect("a token");
    assert!(f.viewer().media_body(token, "viewer-a").is_none());
}

#[test]
fn a_folder_answers_its_typed_error_not_not_found() {
    let f = fixture();
    let error = f
        .call(
            "viewer_open",
            json!({ "path": "/docs", "volumeId": "root", "windowLabel": "viewer-1", "rows": "raw" }),
        )
        .expect_err("a folder doesn't open");
    assert_ne!(error["kind"], "notFound", "{error}");
}

#[test]
fn saving_a_selection_is_refused_typed() {
    let f = fixture();
    let error = f
        .call("viewer_write_range_to_file", json!({}))
        .expect_err("the demo saves nothing");
    assert_eq!(error, json!({ "kind": "destinationIsReadOnly" }));
}
