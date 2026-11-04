//! Where a link in a rendered Markdown document goes: the web or a mail app, a spot in
//! the same document, another file on the document's volume, or nowhere we follow.

use std::path::Path;

use super::markdown_links::{MarkdownLink, resolve_link};

const DOCUMENT: &str = "/Users/a/proj/docs/README.md";

fn resolve(href: &str) -> MarkdownLink {
    resolve_link(href, Path::new(DOCUMENT), Path::new("/"))
}

fn file(path: &str, element_id: Option<&str>) -> MarkdownLink {
    MarkdownLink::File {
        path: path.to_string(),
        element_id: element_id.map(str::to_string),
    }
}

fn anchor(element_id: Option<&str>) -> MarkdownLink {
    MarkdownLink::Anchor {
        element_id: element_id.map(str::to_string),
    }
}

#[test]
fn a_relative_link_opens_the_file_beside_the_document() {
    assert_eq!(resolve("setup.md"), file("/Users/a/proj/docs/setup.md", None));
    assert_eq!(
        resolve("./guides/setup.md"),
        file("/Users/a/proj/docs/guides/setup.md", None)
    );
    assert_eq!(resolve("../LICENSE"), file("/Users/a/proj/LICENSE", None));
}

#[test]
fn a_relative_link_keeps_its_fragment_as_the_rendered_heading_id() {
    assert_eq!(
        resolve("../README.md#install"),
        file("/Users/a/proj/README.md", Some("md-install"))
    );
}

#[test]
fn a_fragment_and_a_path_decode_their_escapes_and_the_query_goes() {
    assert_eq!(
        resolve("my%20notes.md?plain=1#%C3%A1rv%C3%ADz"),
        file("/Users/a/proj/docs/my notes.md", Some("md-árvíz"))
    );
}

#[test]
fn a_link_to_the_document_itself_is_an_anchor() {
    assert_eq!(resolve("README.md#usage"), anchor(Some("md-usage")));
    assert_eq!(resolve("./README.md"), anchor(None));
}

#[test]
fn a_bare_fragment_is_already_the_rendered_id() {
    // The renderer rewrote `#usage` to `#md-usage` in the HTML the click came from.
    assert_eq!(resolve("#md-usage"), anchor(Some("md-usage")));
    assert_eq!(resolve("#md-%C3%A1rv"), anchor(Some("md-árv")));
}

#[test]
fn climbing_past_the_volume_root_stops_at_the_root() {
    assert_eq!(resolve("../../../../../../etc/passwd"), file("/etc/passwd", None));
    let on_share = resolve_link(
        "../../../../notes.md",
        Path::new("/Volumes/share/docs/README.md"),
        Path::new("/Volumes/share"),
    );
    assert_eq!(on_share, file("/Volumes/share/notes.md", None));
}

#[test]
fn a_document_on_a_phone_or_in_an_archive_links_within_it() {
    let on_phone = resolve_link(
        "../Docs/b.md",
        Path::new("adb://R58M1/sdcard/Notes/a.md"),
        Path::new("adb://R58M1"),
    );
    assert_eq!(on_phone, file("adb://R58M1/sdcard/Docs/b.md", None));
    let in_zip = resolve_link("b.md#x", Path::new("/Users/a/x.zip/docs/a.md"), Path::new("/"));
    assert_eq!(in_zip, file("/Users/a/x.zip/docs/b.md", Some("md-x")));
}

#[test]
fn web_and_mail_links_go_out() {
    assert_eq!(
        resolve("https://getcmdr.com/docs?a=1#top"),
        MarkdownLink::External {
            url: "https://getcmdr.com/docs?a=1#top".to_string()
        }
    );
    assert_eq!(
        resolve("HTTP://Example.com"),
        MarkdownLink::External {
            url: "http://example.com/".to_string()
        }
    );
    assert_eq!(
        resolve("mailto:hello@getcmdr.com"),
        MarkdownLink::External {
            url: "mailto:hello@getcmdr.com".to_string()
        }
    );
    assert_eq!(
        resolve("//example.com/a"),
        MarkdownLink::External {
            url: "https://example.com/a".to_string()
        }
    );
}

#[test]
fn absolute_paths_files_and_other_schemes_are_not_followed() {
    for href in [
        "",
        "   ",
        "/docs/setup.md",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "tel:+46700000000",
        "data:text/html,hi",
        "smb://server/share/a.md",
    ] {
        assert_eq!(resolve(href), MarkdownLink::Blocked, "{href:?}");
    }
}
