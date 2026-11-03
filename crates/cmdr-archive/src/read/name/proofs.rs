//! Kani proofs of the sanitizer's decision ([`walk_components`]), for every name
//! up to [`LEN`] bytes. Run them with `pnpm check kani`.
//!
//! **Why four symbols are every name**: the walk only ever asks whether a byte
//! is `/`, `\`, or `.`. Every other byte, a multi-byte UTF-8 char's included,
//! takes the same path as `a`, so names over `{a, ., /, \}` reach every branch
//! and every outcome a real name can.
//!
//! **Assumptions**: names of at most [`LEN`] bytes (so at most eight
//! components), and a depth cap of [`MAX_DEPTH`] standing in for
//! [`MAX_COMPONENT_DEPTH`], which no name this short could reach. The cap
//! compares a count, so a small one covers the branch the same way.
//!
//! What follows for the whole sanitizer: every kept component is a plain name,
//! so the accepted path (the kept components joined with `/`) stays under any
//! root it's joined to, and sanitizing it again keeps the same components. The
//! `archive_entry_name` fuzz target checks both on the real `String`.

use super::*;

/// The size bound: every name of up to this many bytes. At 12 the two proofs
/// take 21 s and 73 s on the M1 Max (Kani 0.68.0, 2026-11-03), and each four
/// bytes more roughly doubles that.
const LEN: usize = 12;
/// The depth cap the proofs pass for [`MAX_COMPONENT_DEPTH`].
const MAX_DEPTH: usize = 3;

/// Any name of up to `LEN` bytes over the four symbols the walk tells apart.
fn any_name(bytes: &mut [u8; LEN]) -> &str {
    *bytes = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= LEN);
    for b in bytes.iter() {
        kani::assume(matches!(*b, b'a' | b'.' | b'/' | b'\\'));
    }
    // SAFETY: every byte is one of four ASCII symbols, so the slice is UTF-8.
    // `from_utf8` would hold the same, at twice the proof's time.
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}

fn is_separator(b: u8) -> bool {
    b == b'/' || b == b'\\'
}

/// Whether `name` has a `..` component, read straight off its bytes: two dots
/// with a separator or an end on each side.
fn has_parent_component(name: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < name.len() {
        if name[i] == b'.'
            && name[i + 1] == b'.'
            && (i == 0 || is_separator(name[i - 1]))
            && (i + 2 == name.len() || is_separator(name[i + 2]))
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Every component the walk keeps is a plain name: never empty, `.`, or `..`,
/// and free of both separators.
#[kani::proof]
#[kani::unwind(14)]
fn kept_components_are_plain_names() {
    let mut bytes = [0; LEN];
    let name = any_name(&mut bytes);
    let _ = walk_components(name, MAX_DEPTH, |component| {
        let component = component.as_bytes();
        assert!(!component.is_empty(), "kept an empty component");
        assert!(component != b"." && component != b"..", "kept a dot component");
        for &b in component {
            assert!(!is_separator(b), "kept a component with a separator in it");
        }
    });
}

/// The verdict matches the name: traversal exactly when a `..` component is
/// there, empty exactly when nothing is kept, too deep exactly past the cap.
#[kani::proof]
#[kani::unwind(14)]
fn the_verdict_matches_the_name() {
    let mut bytes = [0; LEN];
    let name = any_name(&mut bytes);
    let mut kept = 0;
    let verdict = walk_components(name, MAX_DEPTH, |_| kept += 1);
    let traversal = has_parent_component(name.as_bytes());
    assert_eq!(
        verdict == Err(QuarantineReason::ParentTraversal),
        traversal,
        "a `..` component and the traversal verdict disagree"
    );
    if !traversal {
        assert_eq!(
            verdict == Err(QuarantineReason::Empty),
            kept == 0,
            "an empty verdict with components kept"
        );
        assert_eq!(
            verdict == Err(QuarantineReason::TooDeep),
            kept > MAX_DEPTH,
            "the depth cap and the verdict disagree"
        );
    }
}
