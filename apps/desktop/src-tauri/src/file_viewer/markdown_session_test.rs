//! A Markdown file through the viewer's public open path: it opens as a Markdown
//! session, renders, and serves its images from wherever the document lives.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use super::ViewerContentKind;
use super::media::{MediaEntry, MediaSource};
use crate::file_viewer::session_test_support::viewer;
use crate::test_support::TestDir;

/// A PNG signature is all the classifier needs.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n and the rest doesn't matter for classification";

const DOCUMENT: &str = concat!(
    "# Notes\n\n",
    "![pic](img/pic.png)\n\n",
    "![not a picture](secret.txt)\n\n",
    "![web](https://remote.example/x.png)\n",
);

/// The tokens behind the `local:` / `remote:` markers in `html`, in document order.
fn tokens(html: &str, kind: &str) -> Vec<String> {
    let marker = format!("data-md-image=\"{kind}:");
    html.match_indices(&marker)
        .map(|(at, _)| html[at + marker.len()..at + marker.len() + 32].to_string())
        .collect()
}

/// What the `cmdr-media://` handler would serve for `token`, fetching it first.
fn fetch(token: &str) -> Option<MediaEntry> {
    match viewer().tokens().resolve_token(token)? {
        MediaSource::Ready(entry) => Some(entry),
        MediaSource::Deferred(deferred) => deferred.fetch(),
        MediaSource::Volume(_) => None,
    }
}

fn a_local_document(dir: &Path) -> String {
    std::fs::create_dir_all(dir.join("img")).unwrap();
    std::fs::write(dir.join("img/pic.png"), PNG).unwrap();
    std::fs::write(dir.join("secret.txt"), "not an image").unwrap();
    let path = dir.join("README.md");
    std::fs::write(&path, DOCUMENT).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn a_markdown_file_opens_as_a_markdown_session_and_as_text_on_request() {
    let dir = TestDir::new("md_open");
    let path = a_local_document(&dir);

    let rendered = viewer().open_session(&path, "root").expect("opens");
    assert_eq!(rendered.kind, ViewerContentKind::Markdown);
    assert!(
        rendered.initial_lines.rows.is_empty(),
        "a Markdown session has no text rows"
    );
    assert!(rendered.media_token.is_none());

    let text = viewer().open_session_as_text(&path, "root").expect("opens as text");
    assert_eq!(text.kind, ViewerContentKind::Text);
    assert_eq!(text.initial_lines.rows[0].text, "# Notes");

    viewer().close_session(&rendered.session_id).unwrap();
    viewer().close_session(&text.session_id).unwrap();
}

#[test]
fn a_markdown_file_over_the_cap_opens_as_text() {
    let dir = TestDir::new("md_cap");
    let path = dir.join("huge.md");
    let mut file = std::fs::File::create(&path).unwrap();
    let line = [b'a'; 1023].iter().chain(b"\n").copied().collect::<Vec<u8>>();
    for _ in 0..(10 * 1024 + 1) {
        file.write_all(&line).unwrap();
    }
    drop(file);

    let opened = viewer().open_session(path.to_str().unwrap(), "root").expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Text);
    viewer().close_session(&opened.session_id).unwrap();
}

#[test]
fn rendering_serves_local_images_and_only_images_and_holds_remote_ones() {
    let dir = TestDir::new("md_render");
    let path = a_local_document(&dir);
    let opened = viewer().open_session(&path, "root").expect("opens");

    let render = viewer().render_markdown_session(&opened.session_id).expect("renders");
    assert!(render.html.contains(r#"<h1 id="md-notes">"#), "{}", render.html);
    assert!(render.has_remote_images);
    let local = tokens(&render.html, "local");
    let remote = tokens(&render.html, "remote");
    assert_eq!((local.len(), remote.len()), (2, 1), "{}", render.html);

    let picture = fetch(&local[0]).expect("the picture is served");
    assert_eq!(picture.mime, "image/png");
    assert_eq!(picture.canonical_path, dir.join("img/pic.png"));
    assert!(fetch(&local[1]).is_none(), "a text file is never served as an image");
    assert!(fetch(&remote[0]).is_none(), "a web image waits for the user's say-so");

    viewer().close_session(&opened.session_id).unwrap();
    for token in local.iter().chain(&remote) {
        assert!(
            viewer().tokens().resolve_token(token).is_none(),
            "closing drops every image token"
        );
    }
}

#[test]
fn a_second_render_replaces_the_first_ones_tokens() {
    let dir = TestDir::new("md_rerender");
    let path = a_local_document(&dir);
    let opened = viewer().open_session(&path, "root").expect("opens");

    let first = tokens(
        &viewer().render_markdown_session(&opened.session_id).unwrap().html,
        "local",
    );
    let second = tokens(
        &viewer().render_markdown_session(&opened.session_id).unwrap().html,
        "local",
    );
    assert!(viewer().tokens().resolve_token(&first[0]).is_none());
    assert!(viewer().tokens().resolve_token(&second[0]).is_some());

    viewer().close_session(&opened.session_id).unwrap();
}

#[test]
fn markdown_calls_on_a_session_that_isnt_markdown_are_refused() {
    let dir = TestDir::new("md_not");
    let path = dir.join("plain.txt");
    std::fs::write(&path, "hello\n").unwrap();
    let opened = viewer().open_session(path.to_str().unwrap(), "root").expect("opens");

    assert!(viewer().render_markdown_session(&opened.session_id).is_err());
    assert!(viewer().allow_remote_images(&opened.session_id).is_err());

    viewer().close_session(&opened.session_id).unwrap();
}

/// Registers a volume the OS can't open (a phone) holding a document and its picture,
/// and returns the document's path.
fn a_document_on_a_phone(id: &str) -> String {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{InMemoryVolume, Volume as _};

    let root = format!("mtp://{id}/1");
    let volume = InMemoryVolume::new("Phone").with_root(&root);
    let document = format!("{root}/Notes/todo.md");
    tauri::async_runtime::block_on(async {
        volume
            .create_file(Path::new(&document), b"# Todo\n\n![shot](../Pictures/shot.png)\n")
            .await?;
        volume
            .create_file(Path::new(&format!("{root}/Pictures/shot.png")), PNG)
            .await
    })
    .expect("seed the phone");
    assert!(!volume.paths_are_os_visible());
    get_volume_manager().register(id, Arc::new(volume));
    document
}

#[test]
fn a_document_on_a_phone_renders_and_pulls_its_pictures_off_the_phone() {
    let id = cmdr_fs::testing::unique_test_id("md-phone");
    let document = a_document_on_a_phone(&id);

    let opened = viewer().open_session(&document, &id).expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Markdown);
    let render = viewer().render_markdown_session(&opened.session_id).expect("renders");
    let local = tokens(&render.html, "local");
    assert_eq!(local.len(), 1, "{}", render.html);

    let picture = fetch(&local[0]).expect("the picture is pulled and served");
    assert_eq!(picture.mime, "image/png");
    assert!(picture.canonical_path.exists(), "served from a local copy");
    assert_eq!(std::fs::read(&picture.canonical_path).unwrap(), PNG);

    viewer().close_session(&opened.session_id).unwrap();
    assert!(!picture.canonical_path.exists(), "the copy goes with the session");
}

/// Registers the local disk as `root` so a path inside a zip routes.
fn ensure_root_volume() {
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_local::LocalPosixVolume;
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

#[test]
fn a_document_inside_a_zip_renders_with_the_pictures_beside_it() {
    use zip::write::SimpleFileOptions;
    ensure_root_volume();
    let dir = TestDir::new("md_zip");
    let archive = dir.join("docs.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("docs/README.md", opts).unwrap();
    writer.write_all(b"![logo](../art/logo.png)\n").unwrap();
    writer.start_file("art/logo.png", opts).unwrap();
    writer.write_all(PNG).unwrap();
    writer.finish().unwrap();

    let document = archive.join("docs/README.md");
    let opened = viewer()
        .open_session(document.to_str().unwrap(), "root")
        .expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Markdown);
    let render = viewer().render_markdown_session(&opened.session_id).expect("renders");
    let local = tokens(&render.html, "local");

    let picture = fetch(&local[0]).expect("the picture is extracted and served");
    assert_eq!(picture.mime, "image/png");
    viewer().close_session(&opened.session_id).unwrap();
}
