//! Where a Markdown image's `src` points: beside the document on its volume, on the
//! web, inline, or nowhere we'll load.

use std::path::{Path, PathBuf};

use super::markdown_images::{ImageTarget, resolve_image_src};

fn resolve(src: &str) -> ImageTarget {
    resolve_image_src(src, Path::new("/Users/a/proj/docs/README.md"), Path::new("/"))
}

fn local(path: &str) -> ImageTarget {
    ImageTarget::Local(PathBuf::from(path))
}

#[test]
fn a_relative_source_resolves_beside_the_document() {
    assert_eq!(resolve("shot.png"), local("/Users/a/proj/docs/shot.png"));
    assert_eq!(resolve("./img/shot.png"), local("/Users/a/proj/docs/img/shot.png"));
    assert_eq!(resolve("../assets/logo.svg"), local("/Users/a/proj/assets/logo.svg"));
}

#[test]
fn a_relative_source_drops_its_query_and_fragment_and_decodes_escapes() {
    assert_eq!(
        resolve("img/my%20shot.png?raw=true#dark"),
        local("/Users/a/proj/docs/img/my shot.png")
    );
}

#[test]
fn climbing_past_the_volume_root_stops_at_the_root() {
    assert_eq!(resolve("../../../../../../../etc/logo.png"), local("/etc/logo.png"));
    let on_share = resolve_image_src(
        "../../../../secret.png",
        Path::new("/Volumes/share/docs/README.md"),
        Path::new("/Volumes/share"),
    );
    assert_eq!(on_share, local("/Volumes/share/secret.png"));
}

#[test]
fn a_document_on_a_phone_resolves_on_the_phone() {
    let target = resolve_image_src(
        "../Pictures/a.jpg",
        Path::new("adb://R58M1/sdcard/Notes/todo.md"),
        Path::new("adb://R58M1"),
    );
    assert_eq!(target, local("adb://R58M1/sdcard/Pictures/a.jpg"));
}

#[test]
fn a_document_inside_an_archive_resolves_inside_it() {
    assert_eq!(
        resolve_image_src("pics/a.png", Path::new("/Users/a/x.zip/docs/README.md"), Path::new("/")),
        local("/Users/a/x.zip/docs/pics/a.png")
    );
}

#[test]
fn web_sources_are_remote() {
    assert_eq!(
        resolve("https://img.shields.io/badge/x.svg"),
        ImageTarget::Remote("https://img.shields.io/badge/x.svg".to_string())
    );
    assert_eq!(
        resolve("HTTP://example.com/a.png"),
        ImageTarget::Remote("http://example.com/a.png".to_string())
    );
    assert_eq!(
        resolve("//cdn.example.com/a.png"),
        ImageTarget::Remote("https://cdn.example.com/a.png".to_string())
    );
}

#[test]
fn an_inline_data_image_stays_inline() {
    assert_eq!(resolve("data:image/png;base64,iVBORw0KGgo="), ImageTarget::Inline);
    assert_eq!(resolve("DATA:image/gif;base64,R0lGOD=="), ImageTarget::Inline);
}

#[test]
fn absolute_paths_and_other_schemes_are_blocked() {
    for src in [
        "",
        "   ",
        "/etc/logo.png",
        "file:///Users/a/secret.png",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "cmdr-media://localhost/0123456789abcdef0123456789abcdef",
        "ftp://example.com/a.png",
    ] {
        assert_eq!(resolve(src), ImageTarget::Blocked, "{src:?} should be blocked");
    }
}

#[test]
fn a_document_outside_its_volume_resolves_nothing() {
    let target = resolve_image_src("a.png", Path::new("/elsewhere/README.md"), Path::new("/Volumes/share"));
    assert_eq!(target, ImageTarget::Blocked);
}
