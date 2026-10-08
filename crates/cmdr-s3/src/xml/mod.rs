//! S3's XML: response bodies in, request bodies out.
//!
//! Responses are small (a listing page is at most 1,000 entries), so each one
//! is read into a tiny element tree first and the parsers walk that. Elements
//! match by LOCAL name: AWS puts everything in the
//! `http://s3.amazonaws.com/doc/2006-03-01/` default namespace, and some
//! compatible servers omit it.

pub(crate) mod build;
mod parse;

pub(crate) use parse::*;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Deeper than any S3 document goes. Refusing deeper input keeps a hostile
/// body from building a tree whose recursive drop overflows the stack.
const MAX_DEPTH: usize = 32;

/// The body isn't the XML we expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Malformed;

/// One element: its local name, its own text, and its child elements.
#[derive(Debug, Default)]
struct Element {
    name: String,
    /// Character data directly inside this element, entities resolved, ❗
    /// untrimmed: a key may begin or end with spaces.
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// A child's raw text, for keys and prefixes.
    fn raw(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.as_str())
    }

    /// A child's trimmed text, for numbers, dates, ids, and tokens. Empty
    /// counts as absent.
    fn value(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.trim()).filter(|t| !t.is_empty())
    }
}

/// Reads `body` into its root element.
fn parse_tree(body: &str) -> Result<Element, Malformed> {
    let mut reader = Reader::from_str(body);
    let mut stack: Vec<Element> = Vec::new();
    loop {
        match reader.read_event().map_err(|_| Malformed)? {
            Event::Start(start) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(Malformed);
                }
                stack.push(Element {
                    name: local_name(start.local_name().as_ref())?,
                    ..Element::default()
                });
            }
            Event::Empty(start) => {
                let element = Element {
                    name: local_name(start.local_name().as_ref())?,
                    ..Element::default()
                };
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => return Ok(element),
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or(Malformed)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => return Ok(element),
                }
            }
            // A text node never holds an entity: quick-xml 0.41 splits
            // `a&amp;b` into `Text(a)`, `GeneralRef(amp)`, `Text(b)`
            // (`crates/cmdr-webdav/src/propfind.rs` has the evidence). Resolving
            // the reference is what keeps the `&` in a key.
            Event::Text(text) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&text.decode().map_err(|_| Malformed)?);
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(top) = stack.last_mut() {
                    if let Ok(Some(c)) = reference.resolve_char_ref() {
                        top.text.push(c);
                    } else {
                        let name = reference.decode().map_err(|_| Malformed)?;
                        top.text.push_str(resolve_predefined_entity(&name).ok_or(Malformed)?);
                    }
                }
            }
            Event::CData(data) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&data.decode().map_err(|_| Malformed)?);
                }
            }
            Event::Eof => return Err(Malformed),
            Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => {}
        }
    }
}

fn local_name(bytes: &[u8]) -> Result<String, Malformed> {
    std::str::from_utf8(bytes).map(str::to_string).map_err(|_| Malformed)
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tree_test;
