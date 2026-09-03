//! One content fingerprint helper, shared by the policy stamps the disposable
//! caches persist.
//!
//! Two subsystems need the same thing: a stable, on-disk value derived from a set
//! of compile-time constants, so that editing those constants re-arms every
//! existing database with no version number for anyone to forget to bump. The
//! index's scan-exclusion policy (`indexing::scanner::exclusion_policy_fingerprint`)
//! is one; the importance subsystem's scoring policy
//! (`importance::classify::scoring_policy_fingerprint`) is the other. They share
//! this helper rather than each carrying a copy, so there's one mixing function
//! with one golden test behind both stamps.

/// FNV-1a over newline-separated parts, as 16 hex digits.
///
/// Split out from the callers so the mixing is testable against a fixed input. A
/// fingerprint the caller feeds its own constants to can only be tested
/// symmetrically — stamp with it, read with it, agree — and that agrees just as
/// happily with a broken hash that collides two different policies into one value,
/// which would silently skip the re-walk (or the rescore) the whole mechanism
/// exists for. `the_fingerprint_mixes_its_input` pins this against a golden over a
/// test-only input, so it needs no maintenance when the real lists change.
///
/// FNV-1a rather than `DefaultHasher` because the value goes to disk and must not
/// shift with a toolchain upgrade.
pub(crate) fn fingerprint_of(parts: &[&str]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash actually MIXES, pinned against a golden over a fixed input.
    ///
    /// Everything else about a policy stamp is symmetric — write it, read it,
    /// compare — and a hash that collided two different policies into one value
    /// would pass every one of those tests while silently skipping the work a
    /// policy change is supposed to trigger. The input here is test-only, so the
    /// golden never needs touching when the real lists change.
    #[test]
    fn the_fingerprint_mixes_its_input() {
        assert_eq!(fingerprint_of(&["a", "b"]), "78ed6781f136a14e");
        assert_ne!(
            fingerprint_of(&["a", "b"]),
            fingerprint_of(&["b", "a"]),
            "order has to matter, or moving a name between lists reads as no change"
        );
        assert_ne!(
            fingerprint_of(&["a", "b"]),
            fingerprint_of(&["ab"]),
            "the separator has to matter, or two lists concatenate ambiguously"
        );
    }
}
