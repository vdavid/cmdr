//! The names top-level sources take at a transfer's destination when they
//! aren't their own: a rename that runs as a move.
//!
//! A rename is one cheap call on most volumes, but not on an object store,
//! where it copies every byte and deletes the source (`Volume::rename_work`).
//! Such a rename goes through the move engine, whose destination is a FOLDER;
//! this map is what makes "move `/a/foo` into `/a` as `bar`" expressible. It's
//! carried on the operation's state (`WriteOperationState::target_names`) and
//! read in exactly one place, the async transfer driver, where every volume
//! engine builds its top-level destination paths.
//!
//! ❗ A name, never a path: [`TargetNames::new`] refuses a name that isn't one
//! plain path component, so a map can't send a source anywhere but into the
//! destination folder it was given.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

/// The NAME each renamed top-level source takes at the destination, by source
/// path.
// DEFAULT-OK: an empty map is the ordinary transfer, every source keeping its
// own name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TargetNames(HashMap<PathBuf, OsString>);

/// A target name that isn't one plain path component (empty, `.`, `..`, or
/// holding a separator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotAName(pub(crate) String);

impl TargetNames {
    /// The map for `renames`, refusing any name that isn't one plain component.
    pub(crate) fn new(renames: impl IntoIterator<Item = (PathBuf, String)>) -> Result<Self, NotAName> {
        let mut names = HashMap::new();
        for (source, name) in renames {
            let mut components = Path::new(&name).components();
            match (components.next(), components.next()) {
                (Some(Component::Normal(_)), None) if !name.contains('/') => {
                    names.insert(source, OsString::from(name));
                }
                _ => return Err(NotAName(name)),
            }
        }
        Ok(Self(names))
    }

    /// The name `source` takes at the destination: its target name, else its
    /// own.
    pub(crate) fn name_for<'a>(&'a self, source: &'a Path) -> Option<&'a OsStr> {
        self.0
            .get(source)
            .map(OsString::as_os_str)
            .or_else(|| source.file_name())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renamed_source_takes_its_target_name_and_the_rest_keep_their_own() {
        let names = TargetNames::new([(PathBuf::from("/a/foo"), "bar".to_string())]).unwrap();
        assert_eq!(names.name_for(Path::new("/a/foo")), Some(OsStr::new("bar")));
        assert_eq!(names.name_for(Path::new("/a/other")), Some(OsStr::new("other")));
        assert!(TargetNames::default().is_empty());
    }

    /// ❗ A name can't smuggle in a path: the source would land outside the
    /// folder the move was given.
    #[test]
    fn a_target_name_that_isnt_one_plain_component_is_refused() {
        for bad in ["", ".", "..", "x/y", "/abs", "../up"] {
            assert_eq!(
                TargetNames::new([(PathBuf::from("/a/foo"), bad.to_string())]),
                Err(NotAName(bad.to_string())),
                "{bad:?}"
            );
        }
    }
}
