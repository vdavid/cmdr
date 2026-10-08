//! The element tree every parser stands on.

use super::{MAX_DEPTH, Malformed, parse_tree};

#[test]
fn elements_match_by_local_name_whatever_the_namespace_prefix() {
    let root =
        parse_tree(r#"<s3:Root xmlns:s3="http://s3.amazonaws.com/doc/2006-03-01/"><s3:Key>a</s3:Key></s3:Root>"#)
            .unwrap();
    assert_eq!(root.name, "Root");
    assert_eq!(root.raw("Key"), Some("a"));
}

#[test]
fn entities_and_character_references_resolve_inside_text() {
    let root = parse_tree("<R><Key>a&amp;b &lt;c&gt; &quot;d&quot; &#233;&#x41;</Key></R>").unwrap();
    assert_eq!(root.raw("Key"), Some("a&b <c> \"d\" éA"));
}

#[test]
fn text_keeps_its_leading_and_trailing_spaces_because_keys_can_have_them() {
    let root = parse_tree("<R><Key>  spaced name  </Key><Size> 12 </Size></R>").unwrap();
    assert_eq!(root.raw("Key"), Some("  spaced name  "));
    assert_eq!(root.value("Size"), Some("12"));
}

#[test]
fn cdata_is_text() {
    let root = parse_tree("<R><Key><![CDATA[a<b]]></Key></R>").unwrap();
    assert_eq!(root.raw("Key"), Some("a<b"));
}

#[test]
fn an_empty_element_is_present_with_no_text() {
    let root = parse_tree("<R><Owner/><Key></Key></R>").unwrap();
    assert!(root.child("Owner").is_some());
    assert_eq!(root.raw("Key"), Some(""));
    assert_eq!(root.value("Key"), None);
}

#[test]
fn a_self_closing_root_parses() {
    assert_eq!(parse_tree(r#"<?xml version="1.0"?><Empty/>"#).unwrap().name, "Empty");
}

#[test]
fn truncated_or_garbage_bodies_are_malformed() {
    assert_eq!(parse_tree("<R><Key>a</Key>").err(), Some(Malformed));
    assert_eq!(parse_tree("").err(), Some(Malformed));
    assert_eq!(parse_tree("<R><Key>&bogus;</Key></R>").err(), Some(Malformed));
    assert_eq!(
        parse_tree("<html><body>Bad gateway</p></body></html>").err(),
        Some(Malformed)
    );
}

#[test]
fn nesting_past_the_depth_cap_is_refused() {
    let deep = "<a>".repeat(MAX_DEPTH + 1) + &"</a>".repeat(MAX_DEPTH + 1);
    assert_eq!(parse_tree(&deep).err(), Some(Malformed));
    let fine = "<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH);
    assert!(parse_tree(&fine).is_ok());
}
