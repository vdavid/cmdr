//! A handshake refusal found by its typed kind, however deep it sits.

use std::fmt;
use std::io;

use super::has_tls_refusal;

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
