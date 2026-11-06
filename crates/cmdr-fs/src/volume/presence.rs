//! [`Presence`]: what an existence probe learned, "couldn't tell" included.

use super::VolumeError;

/// Whether a path is there, as far as its volume could say
/// ([`Volume::presence`](super::Volume::presence)).
///
/// Three answers, because a probe has three outcomes: the entry answered, the
/// volume said it isn't there, or something kept the question from being
/// answered. Folding the third into "not there" is how a folder the account
/// may not read walked the pane to its parent as if it were deleted (#1018).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Something is at the path.
    Present,
    /// The volume said nothing is at the path.
    Absent,
    /// The volume couldn't say: access was refused, the session dropped, the
    /// container is locked, or the probe failed some other way.
    Unknown,
}

impl Presence {
    /// What a stat's answer says about presence: an entry is there, only
    /// [`VolumeError::NotFound`] is a definite miss, and every other error
    /// (a refusal included) says nothing about the path.
    pub fn of<T>(stat: &Result<T, VolumeError>) -> Self {
        match stat {
            Ok(_) => Self::Present,
            Err(VolumeError::NotFound(_)) => Self::Absent,
            Err(_) => Self::Unknown,
        }
    }

    /// `true` only for [`Self::Present`]: the plain yes/no of
    /// [`Volume::exists`](super::Volume::exists).
    pub fn is_present(self) -> bool {
        self == Self::Present
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_not_found_is_absent_and_a_refusal_is_unknown() {
        assert_eq!(Presence::of(&Ok::<(), VolumeError>(())), Presence::Present);
        assert_eq!(
            Presence::of::<()>(&Err(VolumeError::NotFound("/a".into()))),
            Presence::Absent
        );
        let refused = VolumeError::PermissionDenied {
            path: "/a".into(),
            raw_os_error: None,
        };
        assert_eq!(Presence::of::<()>(&Err(refused)), Presence::Unknown);
        assert_eq!(
            Presence::of::<()>(&Err(VolumeError::NotConnected("/a".into()))),
            Presence::Unknown
        );
    }
}
