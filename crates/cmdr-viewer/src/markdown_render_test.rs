//! Rendering a Markdown document to the sanitized HTML the viewer shows.

use std::sync::{Arc, Mutex};

use super::markdown_render::{ImageMarker, render_markdown};

/// Renders with every image marked by a test rule: `remote` in the source → a remote
/// token, `data:` → inline, `file:` → blocked, anything else → a local token. Returns the
/// HTML and the sources the renderer asked about, in order.
fn render(source: &str) -> (String, Vec<String>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let html = render_markdown(source, move |src: &str| {
        let mut seen = seen.lock().unwrap();
        seen.push(src.to_string());
        let n = seen.len();
        if src.starts_with("data:") {
            ImageMarker::Inline
        } else if src.contains("remote") {
            ImageMarker::Remote(format!("r{n}"))
        } else if src.starts_with("file:") {
            ImageMarker::Blocked
        } else {
            ImageMarker::Local(format!("l{n}"))
        }
    });
    let asked = asked.lock().unwrap().clone();
    (html, asked)
}

fn html(source: &str) -> String {
    render(source).0
}

#[test]
fn headings_get_github_style_ids_under_a_prefix() {
    let out = html("# Getting Started!\n\n## Getting started\n\n### API: `run()`\n\n## Custom {#own-id}\n");
    assert!(out.contains(r#"<h1 id="md-getting-started">"#), "{out}");
    assert!(out.contains(r#"<h2 id="md-getting-started-1">"#), "{out}");
    assert!(out.contains(r#"<h3 id="md-api-run">"#), "{out}");
    assert!(out.contains(r#"<h2 id="md-own-id">"#), "{out}");
}

#[test]
fn in_page_links_point_at_the_prefixed_ids() {
    let out = html("[Install](#install)\n\nText[^note].\n\n[^note]: A footnote.\n");
    assert!(out.contains(r##"href="#md-install""##), "{out}");
    assert!(out.contains(r##"href="#md-note""##), "{out}");
    assert!(out.contains(r#"id="md-note""#), "{out}");
    assert!(out.contains(r#"class="footnote-reference""#), "{out}");
    assert!(out.contains(r#"class="footnote-definition""#), "{out}");
}

#[test]
fn github_flavored_extensions_render() {
    let out = html("| a | b |\n|:-:|--:|\n| 1 | 2 |\n\n~~gone~~\n\n- [x] done\n- [ ] todo\n\n> [!NOTE]\n> Heads up.\n");
    assert!(out.contains("<thead>"), "{out}");
    assert!(
        out.contains(r#"<th style="text-align:center">"#) || out.contains(r#"<th style="text-align: center">"#),
        "{out}"
    );
    assert!(out.contains("<del>gone</del>"), "{out}");
    assert!(out.contains(r#"type="checkbox""#), "{out}");
    assert!(out.contains("checked"), "{out}");
    assert!(out.contains(r#"class="markdown-alert-note""#), "{out}");
}

#[test]
fn task_list_checkboxes_are_read_only() {
    let out = html("- [ ] todo\n\n<input type=\"checkbox\">\n");
    assert_eq!(out.matches("<input").count(), 2, "{out}");
    assert_eq!(out.matches("disabled").count(), 2, "{out}");
}

#[test]
fn code_blocks_stay_plain() {
    let out = html("```rust\nfn main() {}\n```\n");
    assert!(out.contains("<pre><code>fn main() {}\n</code></pre>"), "{out}");
}

#[test]
fn scripts_handlers_and_dangerous_tags_are_stripped() {
    let out = html(concat!(
        "<script>alert(1)</script>\n\n",
        "<p onclick=\"alert(2)\" style=\"position:fixed\">hi</p>\n\n",
        "<a href=\"javascript:alert(3)\">js</a>\n\n",
        "<iframe src=\"https://evil.example\"></iframe>\n\n",
        "<form action=\"https://evil.example\"><input type=\"text\" name=\"q\"><button>Go</button></form>\n\n",
        "<style>body { display: none }</style>\n\n",
        "<object data=\"x.swf\"></object><embed src=\"x.swf\">\n\n",
        "[link](javascript:alert(4))\n",
    ));
    for banned in [
        "<script",
        "alert(1)",
        "onclick",
        "position:fixed",
        "javascript:",
        "<iframe",
        "<form",
        "<style",
        "display: none",
        "<object",
        "<embed",
        "<button",
        r#"type="text""#,
    ] {
        assert!(!out.contains(banned), "{banned} survived: {out}");
    }
    assert!(out.contains("hi"), "{out}");
}

#[test]
fn readme_html_survives() {
    let out = html(concat!(
        "<p align=\"center\"><b>Bold</b> <kbd>Cmd</kbd></p>\n\n",
        "<details open><summary>More</summary>\n\nHidden *text*.\n\n</details>\n",
    ));
    assert!(out.contains(r#"<p align="center">"#), "{out}");
    assert!(out.contains("<kbd>Cmd</kbd>"), "{out}");
    assert!(out.contains("<details open"), "{out}");
    assert!(out.contains("<summary>More</summary>"), "{out}");
    assert!(out.contains("<em>text</em>"), "{out}");
}

#[test]
fn images_lose_their_source_and_carry_a_marker_instead() {
    let (out, asked) = render(concat!(
        "![Shot](./shot.png \"A title\")\n\n",
        "<img src=\"../logo.svg\" width=\"120\" alt=\"Logo\">\n\n",
        "![Badge](https://remote.example/badge.svg)\n\n",
        "![Dot](data:image/png;base64,iVBORw0KGgo=)\n\n",
        "<img src=\"file:///etc/logo.png\">\n\n",
        "<img alt=\"no source\">\n",
    ));
    assert_eq!(
        asked,
        vec![
            "./shot.png",
            "../logo.svg",
            "https://remote.example/badge.svg",
            "data:image/png;base64,iVBORw0KGgo="
        ],
        "the renderer asks about each source in document order"
    );
    assert!(out.contains(r#"data-md-image="local:l1""#), "{out}");
    assert!(out.contains(r#"data-md-image="local:l2""#), "{out}");
    assert!(out.contains(r#"data-md-image="remote:r3""#), "{out}");
    assert!(out.contains(r#"src="data:image/png;base64,iVBORw0KGgo=""#), "{out}");
    assert_eq!(
        out.matches(r#"data-md-image="blocked""#).count(),
        2,
        "file: and source-less images: {out}"
    );
    assert_eq!(
        out.matches(" src=").count(),
        1,
        "only the inline image keeps a source: {out}"
    );
    assert!(out.contains(r#"width="120""#), "{out}");
    assert!(out.contains(r#"alt="Shot""#), "{out}");
}

#[test]
fn a_marker_written_in_the_document_is_replaced_by_the_renderers_own() {
    let (out, _) = render("<img data-md-image=\"local:stolen\" src=\"a.png\">\n");
    assert!(!out.contains("stolen"), "{out}");
    assert!(out.contains(r#"data-md-image="local:l1""#), "{out}");
}
