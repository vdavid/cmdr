//! The Zip Slip choke point: no name `sanitize_entry_name` accepts may leave the
//! directory it's joined under, and accepting is idempotent.
#![no_main]

use std::path::{Component, Path};

use cmdr_archive::{SanitizedName, sanitize_entry_name};

libfuzzer_sys::fuzz_target!(|raw: &str| {
    let SanitizedName::Accepted(inner) = sanitize_entry_name(raw) else {
        return;
    };
    assert!(!inner.is_empty(), "{raw:?} was accepted as an empty path");
    assert!(!inner.contains('\\'), "{raw:?} kept a backslash: {inner:?}");
    // The escape check runs first: the component check below would also catch a
    // `..` or a leading `/`, and run first it left this one unreachable.
    for component in Path::new("/root").join(&inner).components().skip(2) {
        assert!(
            matches!(component, Component::Normal(_)),
            "{raw:?} escapes its root as {inner:?}"
        );
    }
    // What `Path` normalizes away, so only this check sees it: a `.` or an empty
    // component mid-path.
    for component in inner.split('/') {
        assert!(
            !matches!(component, "" | "." | ".."),
            "{raw:?} was accepted with a {component:?} component: {inner:?}"
        );
    }
    assert_eq!(
        sanitize_entry_name(&inner),
        SanitizedName::Accepted(inner.clone()),
        "not idempotent"
    );
});
