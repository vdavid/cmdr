//! A listed entry's name, proven to be one plain path component before anything
//! joins it onto a destination.
//!
//! A listing's names come from whoever answers it: a server, a phone, an archive.
//! `Path::join` trusts them completely: `dest.join("../x")` climbs out of `dest`,
//! and `dest.join("/x")` throws `dest` away. So every engine that turns a SOURCE
//! listing into a DESTINATION path takes a [`ChildName`], and the only way to get
//! one is [`ChildName::new`], which refuses anything but a single plain name.
//!
//! What it refuses: the empty name, `.`, `..`, any `/`, and NUL. An absolute
//! name starts with `/`, so it's covered. What it keeps:
//!
//! - `:` is an ordinary character at the POSIX layer every write goes through.
//!   Finder shows it as `/` (the old HFS separator swap), but the kernel never
//!   splits on it, so `a:b` stays one file named `a:b`.
//! - `\` is ordinary on macOS too. An SMB destination, where it IS a separator,
//!   never sees it raw: `smb2` maps it into the private-use area like every other
//!   character SMB forbids (`VolumeError::InvalidName`'s doc).
//!
//! A refusal is [`VolumeError::InvalidName`](super::VolumeError::InvalidName)
//! (through [`NotAChildName`], whose `From` lives beside `VolumeError`), the
//! same typed "this name can't land here" every backend already raises, so the
//! transfer engine fails the item the way it fails any other unusable name.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::log_error::{ErrorLogText, ErrorText, write_plain};
use crate::log_private::LogPrivate;

/// One plain path component: never empty, `.`, or `..`, and never holding a
/// `/` or a NUL. Built only by [`ChildName::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildName<'a>(&'a OsStr);

/// A listed name that isn't one plain path component. Carries the name, lossily
/// decoded, for the error's technical details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAChildName(pub String);

impl<'a> ChildName<'a> {
    /// `name`, if it's one plain path component.
    pub fn new(name: &'a (impl AsRef<OsStr> + ?Sized)) -> Result<Self, NotAChildName> {
        let name = name.as_ref();
        let bytes = name.as_encoded_bytes();
        let plain = !matches!(bytes, b"" | b"." | b"..") && !bytes.iter().any(|&b| b == b'/' || b == 0);
        if plain {
            Ok(Self(name))
        } else {
            Err(NotAChildName(name.to_string_lossy().into_owned()))
        }
    }

    /// The name itself.
    pub fn as_os_str(self) -> &'a OsStr {
        self.0
    }

    /// The name as UTF-8, when it is.
    pub fn to_str(self) -> Option<&'a str> {
        self.0.to_str()
    }

    /// `dir/<name>`: the one join a listed name may take, and always a direct
    /// child of `dir`.
    pub fn under(self, dir: &Path) -> PathBuf {
        dir.join(self.0)
    }
}

impl fmt::Display for NotAChildName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`: the refused name is marked, inside the quotes
/// its plain spelling keeps.
impl ErrorLogText for NotAChildName {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result {
        fmt::Write::write_str(out, "not a single file name: ")?;
        out.quoted_value(LogPrivate::name(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume::VolumeError;

    #[test]
    fn a_plain_name_passes_and_joins_as_a_direct_child() {
        for good in ["photo.jpg", "a:b", "back\\slash", "...", ".hidden", "with space", "ünï"] {
            let name = ChildName::new(good).unwrap_or_else(|e| panic!("{good:?} refused: {e}"));
            assert_eq!(
                name.under(Path::new("/dest")).parent(),
                Some(Path::new("/dest")),
                "{good:?}"
            );
        }
    }

    /// ❗ Each of these, joined onto a destination, lands somewhere else or
    /// nowhere: above it, at the root, in a subfolder, or on the folder itself.
    #[test]
    fn a_name_that_isnt_one_plain_component_is_refused() {
        for bad in ["", ".", "..", "../x", "/x", "/", "a/b", "a/", "x\0y"] {
            assert_eq!(ChildName::new(bad), Err(NotAChildName(bad.to_string())), "{bad:?}");
        }
    }

    #[test]
    fn a_refusal_is_the_typed_invalid_name() {
        let err: VolumeError = ChildName::new("../x").unwrap_err().into();
        assert!(matches!(err, VolumeError::InvalidName(_)), "{err:?}");
    }

    /// The refused name is someone's file name: a log line marks it, inside the quotes its
    /// plain spelling keeps, so stripped of its markers the line reads as `Display`.
    #[test]
    fn a_refused_name_is_marked_in_a_log_line_through_the_volume_error() {
        use crate::log_error::LogError;
        use crate::log_private::{LogPrivate, RESERVED};

        let name = "Lake \"House\"/IMG 0001.jpg";
        let err: VolumeError = ChildName::new(name).unwrap_err().into();
        let logged = LogError(&err).to_string();
        let span = format!("{:?}", LogPrivate::name(name));
        assert!(logged.contains(&span), "{logged:?}");
        let plain: String = logged.chars().filter(|c| !RESERVED.contains(c)).collect();
        assert_eq!(plain, err.to_string());
        assert_eq!(
            err.to_string(),
            format!("Name not usable at the destination: not a single file name: {name:?}")
        );
    }
}
