//! The user flags a copy carries: a small typed set over the BSD `st_flags`
//! bits, so nothing outside this file handles a raw flag word.

use std::ops::BitOr;

/// A set of the user-settable flags Finder shows. `SF_*` system flags are never
/// in it: a non-root process can't set or clear them, and a copy never tries.
// DEFAULT-OK: the empty set says "no flags to add or clear", which is what a
// default means wherever one is built: nothing claims the item has none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserFlags(u8);

impl UserFlags {
    /// No flags.
    pub const NONE: Self = Self(0);
    /// `UF_HIDDEN`: Finder's Hidden.
    pub const HIDDEN: Self = Self(1);
    /// `UF_IMMUTABLE` (`uchg`): Finder's Locked. Refuses a rename and an unlink.
    pub const LOCKED: Self = Self(1 << 1);
    /// `UF_APPEND` (`uappnd`). Refuses a rename and an unlink like Locked.
    pub const APPEND_ONLY: Self = Self(1 << 2);
    /// `UF_NODUMP`.
    pub const NO_DUMP: Self = Self(1 << 3);
    /// The flags that refuse a rename or an unlink of the item wearing them.
    pub const RENAME_BLOCKERS: Self = Self(Self::LOCKED.0 | Self::APPEND_ONLY.0);

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every flag in `other` is in this set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The flags of this set that are also in `other`.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// This set without the flags in `other`.
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for UserFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
