//! A handshake refusal found by its typed kind, however deep it sits.

use std::fmt;
use std::io;

use super::{causes, has_tls_refusal};

/// An error wrapping `source`, the way a client error wraps its cause.
#[derive(Debug)]
struct Wrapping(Box<dyn std::error::Error + 'static>);

impl fmt::Display for Wrapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wrapping")
    }
}

impl std::error::Error for Wrapping {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[test]
fn an_invalid_data_io_error_deep_in_the_chain_is_a_tls_refusal() {
    let refusal = io::Error::new(io::ErrorKind::InvalidData, "invalid peer certificate");
    let err = Wrapping(Box::new(Wrapping(Box::new(refusal))));
    assert!(has_tls_refusal(&err));
}

#[test]
fn a_refusal_inside_another_io_error_is_found() {
    // The shape reqwest 0.13.5 hands back for a self-signed server (printed off
    // `webdav-fixture-tls`, 2026-10-20): the `InvalidData` sits INSIDE an
    // `io::Error` of kind `Other`, whose `source()` skips it.
    let refusal = io::Error::new(io::ErrorKind::InvalidData, "invalid peer certificate");
    let err = Wrapping(Box::new(io::Error::other(refusal)));
    assert!(has_tls_refusal(&err));
}

#[test]
fn causes_descend_into_an_io_errors_wrapped_error() {
    let refusal = io::Error::new(io::ErrorKind::InvalidData, "invalid peer certificate");
    let err = Wrapping(Box::new(io::Error::other(refusal)));
    let kinds: Vec<io::ErrorKind> = causes(&err)
        .filter_map(|cause| cause.downcast_ref::<io::Error>().map(io::Error::kind))
        .collect();
    assert_eq!(kinds, [io::ErrorKind::Other, io::ErrorKind::InvalidData]);
}

#[test]
fn any_other_io_error_is_not() {
    let refused = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
    assert!(!has_tls_refusal(&Wrapping(Box::new(refused))));
}

#[test]
fn the_error_itself_is_not_its_own_source() {
    // Only what sits UNDER the client error counts, the way `reqwest` nests it.
    let top = io::Error::new(io::ErrorKind::InvalidData, "top");
    assert!(!has_tls_refusal(&top));
}
