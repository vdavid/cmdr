//! Markdown → sanitized HTML for the viewer's rendered view. Pure: no I/O.
//!
//! The file is untrusted, and the HTML lands in the viewer's own webview (`{@html}`),
//! so the sanitizer's allowlist is the security boundary: `pulldown-cmark` renders
//! (raw HTML passes through, as READMEs need it), then `ammonia` keeps only known-safe
//! tags and attributes. No script, event handler, `javascript:` URL, form, frame,
//! stylesheet, or `style` attribute survives (bar `text-align`, which tables need).
//!
//! **Images never keep a `src`.** A local image needs a `cmdr-media://` token and a
//! remote one needs the user's say-so, so the renderer asks its caller about each
//! source and writes the answer as a `data-md-image` marker the frontend acts on:
//! `local:<token>`, `remote:<token>`, or `blocked`. Only an inline `data:image/…`
//! keeps its `src`. `DETAILS.md` § "Markdown rendering".
//!
//! **Ids carry the `md-` prefix**, and in-page links (`#install`) are rewritten to
//! match: a heading called "Loading screen" must not collide with the page's own
//! `#loading-screen`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

use cmdr_fs::ignore_poison::IgnorePoison;

/// What an image becomes in the rendered HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImageMarker {
    /// A file on the document's volume, served under this `cmdr-media://` token.
    Local(String),
    /// A web image, served under this token once the user allows remote images.
    Remote(String),
    /// An inline `data:image/…` source, kept as written.
    Inline,
    /// Not loaded: an absolute path, `file:`, another scheme, or no source at all.
    Blocked,
}

impl ImageMarker {
    fn attribute_value(&self) -> Option<String> {
        match self {
            Self::Local(token) => Some(format!("local:{token}")),
            Self::Remote(token) => Some(format!("remote:{token}")),
            Self::Inline => None,
            Self::Blocked => Some("blocked".to_string()),
        }
    }
}

/// The attribute the frontend reads an image's marker from.
const IMAGE_MARKER_ATTRIBUTE: &str = "data-md-image";

/// The prefix every rendered `id` (and every in-page link) carries.
pub(super) const ID_PREFIX: &str = "md-";

/// Renders `source` to sanitized HTML. `image` is asked once per `<img>` that has a
/// source, in document order, and answers what that image becomes.
pub(super) fn render_markdown(source: &str, image: impl Fn(&str) -> ImageMarker + Send + Sync + 'static) -> String {
    let mut unsafe_html = String::with_capacity(source.len() * 3 / 2);
    html::push_html(
        &mut unsafe_html,
        with_heading_ids(Parser::new_ext(source, markdown_options())),
    );
    sanitizer(image).clean(&unsafe_html).to_string()
}

/// GitHub's flavor, minus anything that rewrites the author's text (smart punctuation)
/// or needs a renderer we don't ship (math).
fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
}

/// Gives every heading without an explicit `{#id}` GitHub's slug of its text, numbering
/// repeats (`intro`, `intro-1`), so links written for GitHub land here too.
fn with_heading_ids<'a>(events: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut heading: Option<Vec<Event<'a>>> = None;
    for event in events {
        match (&mut heading, event) {
            (None, event @ Event::Start(Tag::Heading { .. })) => heading = Some(vec![event]),
            (Some(buffered), Event::End(TagEnd::Heading(level))) => {
                let mut buffered = std::mem::take(buffered);
                heading = None;
                let text: String = buffered[1..]
                    .iter()
                    .filter_map(|event| match event {
                        Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                        _ => None,
                    })
                    .collect();
                if let Some(Event::Start(Tag::Heading { id, .. })) = buffered.first_mut()
                    && id.is_none()
                {
                    *id = Some(CowStr::from(unique_slug(&text, &mut used)));
                }
                out.extend(buffered);
                out.push(Event::End(TagEnd::Heading(level)));
            }
            (Some(buffered), event) => buffered.push(event),
            (None, event) => out.push(event),
        }
    }
    out.into_iter()
}

/// GitHub's heading anchor: lowercase, spaces to `-`, punctuation other than `-` and `_`
/// dropped, and a `-N` suffix on the Nth repeat.
fn unique_slug(text: &str, used: &mut HashMap<String, usize>) -> String {
    let base: String = text
        .trim()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect();
    let seen = used.entry(base.clone()).or_insert(0);
    let slug = if *seen == 0 { base } else { format!("{base}-{seen}") };
    *seen += 1;
    slug
}

/// What the attribute filter knows about the `<img>` it's walking. ammonia visits an
/// element's attributes in order and appends the marker attribute LAST (it's a "set"
/// attribute), so the `src` answer is always waiting when the marker comes up.
enum PendingImage {
    /// No `src` seen: none written, or the scheme check already dropped it.
    NoSource,
    /// The `src` is inline and stays; no marker.
    KeepSource,
    Marker(String),
}

/// The allowlist. ammonia's defaults (no script, no handlers, no forms, frames, or
/// `style`, `rel="noopener noreferrer"` on links) plus what Markdown and READMEs need.
fn sanitizer(image: impl Fn(&str) -> ImageMarker + Send + Sync + 'static) -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input"])
        .add_tag_attributes("input", ["checked"])
        .add_tag_attribute_values("input", "type", ["checkbox"])
        .set_tag_attribute_value("input", "disabled", "")
        .add_tag_attributes("details", ["open"])
        .add_tag_attributes("th", ["style"])
        .add_tag_attributes("td", ["style"])
        .filter_style_properties(HashSet::from(["text-align"]))
        .set_tag_attribute_value("img", IMAGE_MARKER_ATTRIBUTE, "")
        .url_schemes(HashSet::from(["http", "https", "mailto", "data"]))
        .id_prefix(Some(ID_PREFIX))
        .allowed_classes(HashMap::from([
            (
                "sup",
                HashSet::from(["footnote-reference", "footnote-definition-label"]),
            ),
            ("div", HashSet::from(["footnote-definition"])),
            (
                "blockquote",
                HashSet::from([
                    "markdown-alert-note",
                    "markdown-alert-tip",
                    "markdown-alert-important",
                    "markdown-alert-warning",
                    "markdown-alert-caution",
                ]),
            ),
        ]));
    for tag in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        builder.add_tag_attributes(tag, ["id", "align"]);
    }
    builder.add_tag_attributes("div", ["id", "align"]);
    builder.add_tag_attributes("p", ["align"]);

    let pending = Arc::new(Mutex::new(PendingImage::NoSource));
    builder.attribute_filter(move |element, attribute, value| match (element, attribute) {
        ("img", "src") => {
            let marker = image(value);
            let keep = matches!(marker, ImageMarker::Inline);
            *pending.lock_ignore_poison() = match marker.attribute_value() {
                Some(marker) => PendingImage::Marker(marker),
                None => PendingImage::KeepSource,
            };
            keep.then(|| value.into())
        }
        ("img", IMAGE_MARKER_ATTRIBUTE) => {
            match std::mem::replace(&mut *pending.lock_ignore_poison(), PendingImage::NoSource) {
                PendingImage::Marker(marker) => Some(marker.into()),
                PendingImage::KeepSource => None,
                PendingImage::NoSource => ImageMarker::Blocked.attribute_value().map(Cow::Owned),
            }
        }
        ("a", "href") => match value.strip_prefix('#') {
            Some(fragment) => Some(format!("#{ID_PREFIX}{fragment}").into()),
            // A `data:` URL is allowed for images only.
            None if value
                .get(..5)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:")) =>
            {
                None
            }
            None => Some(value.into()),
        },
        _ => Some(value.into()),
    });
    builder
}
