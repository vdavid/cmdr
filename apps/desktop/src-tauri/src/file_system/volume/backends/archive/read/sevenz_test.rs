//! Unit tests for [`super::map_sevenz_err`], the sevenz-rust2 → [`ArchiveError`]
//! classifier.
//!
//! The encryption cases are the point: with the `aes256` feature off (our build),
//! a real `7z`-produced encrypted archive makes sevenz-rust2 return
//! `UnsupportedCompressionMethod("AES256_SHA256")` — at `ArchiveReader::new` for a
//! header-encrypted archive (`7z -mhe=on`), at `for_each_entries` for a
//! data-encrypted one (`7z -mhe=off`). Both shapes must classify as `Unsupported`
//! so the user gets the honest "can't open this kind" experience, never "damaged
//! archive". (Empirically confirmed on sevenz-rust2 0.21.2 against both fixtures,
//! 2026-07-08.) We pin the predicate on constructed error values rather than a
//! checked-in encrypted blob: the crate can't encrypt with `aes256` off, so an
//! in-memory encrypted fixture isn't buildable, and shelling out to `7z` would
//! make the test depend on a tool absent from CI.

use sevenz_rust2::Error as E;

use super::{ArchiveError, map_sevenz_err};

#[test]
fn encrypted_7z_classifies_as_unsupported_not_corrupt() {
    // The exact error a real AES-encrypted 7z yields in our `aes256`-off build,
    // for BOTH the header-encrypted (open-time) and data-encrypted (decode-time)
    // shapes.
    let mapped = map_sevenz_err(E::UnsupportedCompressionMethod("AES256_SHA256".to_string()));
    assert!(
        matches!(mapped, ArchiveError::Unsupported(_)),
        "an encrypted 7z must refuse as Unsupported (honest 'can't open this kind'), got {mapped:?}"
    );
    // Guard the two failure modes the honesty fix exists to prevent:
    assert!(
        !matches!(mapped, ArchiveError::Corrupt(_)),
        "must not read as a damaged archive"
    );
    assert!(
        !matches!(mapped, ArchiveError::Encrypted | ArchiveError::WrongPassword),
        "must not trigger a password prompt a 7z read can never satisfy here"
    );
}

#[test]
fn unknown_7z_codec_also_unsupported() {
    let mapped = map_sevenz_err(E::UnsupportedCompressionMethod("SOME_FUTURE_CODEC".to_string()));
    assert!(matches!(mapped, ArchiveError::Unsupported(_)), "got {mapped:?}");
}

#[test]
fn aes256_on_password_shapes_map_to_unsupported() {
    // These only arise with the `aes256` feature ON; mapped defensively so
    // encryption never reads as damaged if the feature is ever enabled.
    assert!(matches!(map_sevenz_err(E::PasswordRequired), ArchiveError::Unsupported(_)));
    let io = std::io::Error::new(std::io::ErrorKind::InvalidData, "bad pw");
    assert!(matches!(map_sevenz_err(E::MaybeBadPassword(io)), ArchiveError::Unsupported(_)));
}

#[test]
fn broken_7z_structure_still_reads_as_corrupt() {
    // The fix must not over-reach: a genuinely damaged archive stays Corrupt.
    assert!(matches!(map_sevenz_err(E::NextHeaderCrcMismatch), ArchiveError::Corrupt(_)));
    assert!(matches!(map_sevenz_err(E::ChecksumVerificationFailed), ArchiveError::Corrupt(_)));
    assert!(matches!(map_sevenz_err(E::BadSignature([0; 6])), ArchiveError::Corrupt(_)));
}

#[test]
fn memory_limit_maps_to_too_large() {
    let mapped = map_sevenz_err(E::MaxMemLimited {
        max_kb: 1024,
        actaul_kb: 4096,
    });
    assert!(matches!(mapped, ArchiveError::TooLarge(_)), "got {mapped:?}");
}

#[test]
fn byte_source_io_classifies_by_io_kind() {
    // A truncated stream (UnexpectedEof) is a damaged archive, not a live fault.
    let eof = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short read");
    assert!(matches!(map_sevenz_err(E::Io(eof, "".into())), ArchiveError::Corrupt(_)));
    // A real I/O fault stays Io.
    let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    assert!(matches!(map_sevenz_err(E::Io(denied, "".into())), ArchiveError::Io(_)));
}
