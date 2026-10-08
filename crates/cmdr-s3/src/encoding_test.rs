//! The encoding rules, by example.

use super::{KeyError, canonical_query, encode_component, encode_key, wire_query};

fn params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn unreserved_bytes_pass_and_everything_else_is_uppercase_hex() {
    assert_eq!(encode_component("AZaz09-._~"), "AZaz09-._~");
    assert_eq!(encode_component("a b+c/d=e&f"), "a%20b%2Bc%2Fd%3De%26f");
    assert_eq!(encode_component("$"), "%24");
    assert_eq!(encode_component("é"), "%C3%A9");
}

#[test]
fn a_key_keeps_its_slashes_and_encodes_each_segment() {
    assert_eq!(encode_key("test$file.text").unwrap(), "test%24file.text");
    assert_eq!(
        encode_key("Photos/2024 trip/a+b.jpg").unwrap(),
        "Photos/2024%20trip/a%2Bb.jpg"
    );
    assert_eq!(encode_key("folder/").unwrap(), "folder/");
    assert_eq!(encode_key("/leading").unwrap(), "/leading");
    assert_eq!(encode_key("a//b").unwrap(), "a//b");
}

#[test]
fn a_key_with_a_dot_segment_is_refused_because_a_url_parser_would_resolve_it() {
    assert_eq!(encode_key("a/../b"), Err(KeyError::DotSegment));
    assert_eq!(encode_key("./a"), Err(KeyError::DotSegment));
    assert_eq!(encode_key("a/."), Err(KeyError::DotSegment));
    assert_eq!(encode_key(".."), Err(KeyError::DotSegment));
    // Dots inside a name are ordinary.
    assert_eq!(encode_key("a/...").unwrap(), "a/...");
    assert_eq!(encode_key(".hidden/..x").unwrap(), ".hidden/..x");
}

#[test]
fn an_empty_key_is_refused() {
    assert_eq!(encode_key(""), Err(KeyError::Empty));
}

#[test]
fn the_canonical_query_sorts_and_writes_valueless_names_with_an_equals_sign() {
    let query = params(&[("prefix", "J"), ("max-keys", "2"), ("uploads", "")]);
    assert_eq!(canonical_query(&query), "max-keys=2&prefix=J&uploads=");
    assert_eq!(wire_query(&query), "max-keys=2&prefix=J&uploads");
}

#[test]
fn query_values_encode_slashes_and_spaces() {
    let query = params(&[("prefix", "a b/c"), ("continuation-token", "1+x=")]);
    assert_eq!(canonical_query(&query), "continuation-token=1%2Bx%3D&prefix=a%20b%2Fc");
}

#[test]
fn an_empty_query_is_empty() {
    assert_eq!(canonical_query(&[]), "");
    assert_eq!(wire_query(&[]), "");
}
