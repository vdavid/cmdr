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
    for component in inner.split('/') {
        assert!(
            !matches!(component, "" | "." | ".."),
            "{raw:?} was accepted with a {component:?} component: {inner:?}"
        );
    }
    for component in Path::new("/root").join(&inner).components().skip(2) {
        assert!(
            matches!(component, Component::Normal(_)),
            "{raw:?} escapes its root as {inner:?}"
        );
    }
    assert_eq!(
        sanitize_entry_name(&inner),
        SanitizedName::Accepted(inner.clone()),
        "not idempotent"
    );
});
