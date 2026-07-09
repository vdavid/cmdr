//! Unit tests for [`super::map_sevenz_err`], the sevenz-rust2 → [`ArchiveError`]
//! classifier. The behaviour against REAL encrypted 7z bytes (the end-to-end
//! decrypt, the browse-time prompt for a header-encrypted archive) is exercised
//! in [`super::multiformat_test`]; these pin the pure variant → typed-error map.
//!
//! The encryption cases are the point: with `aes256` ON, a password-protected 7z
//! surfaces `PasswordRequired` (no password) and `MaybeBadPassword` (wrong one),
//! which must reach the typed `Encrypted` / `WrongPassword` the frontend prompts
//! on — never `Unsupported` (which reads as "can't open this kind") or `Corrupt`
//! ("damaged archive").

use sevenz_rust2::Error as E;

use super::{ArchiveError, map_sevenz_err};

#[test]
fn no_password_maps_to_the_encrypted_prompt_signal() {
    // sevenz-rust2 reports `PasswordRequired` when an encrypted archive is opened
    // (header-encrypted) or decoded (content-encrypted) with no password.
    let mapped = map_sevenz_err(E::PasswordRequired);
    assert!(
        matches!(mapped, ArchiveError::Encrypted),
        "no password must yield the typed needs-password signal, got {mapped:?}"
    );
}

#[test]
fn wrong_password_maps_to_wrong_password() {
    // A supplied password that decrypts to bytes failing their integrity check.
    let io = std::io::Error::new(std::io::ErrorKind::InvalidData, "bad pw");
    let mapped = map_sevenz_err(E::MaybeBadPassword(io));
    assert!(
        matches!(mapped, ArchiveError::WrongPassword),
        "a bad password must yield the typed WrongPassword re-prompt, got {mapped:?}"
    );
}

#[test]
fn unknown_7z_codec_is_unsupported_not_corrupt() {
    // A genuinely-unknown codec is an honest "can't serve this kind", never damaged.
    let mapped = map_sevenz_err(E::UnsupportedCompressionMethod("SOME_FUTURE_CODEC".to_string()));
    assert!(matches!(mapped, ArchiveError::Unsupported(_)), "got {mapped:?}");
    assert!(!matches!(mapped, ArchiveError::Corrupt(_)));
}

#[test]
fn broken_7z_structure_still_reads_as_corrupt() {
    // The fix must not over-reach: a genuinely damaged archive stays Corrupt.
    assert!(matches!(
        map_sevenz_err(E::NextHeaderCrcMismatch),
        ArchiveError::Corrupt(_)
    ));
    assert!(matches!(
        map_sevenz_err(E::ChecksumVerificationFailed),
        ArchiveError::Corrupt(_)
    ));
    assert!(matches!(
        map_sevenz_err(E::BadSignature([0; 6])),
        ArchiveError::Corrupt(_)
    ));
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
    assert!(matches!(
        map_sevenz_err(E::Io(eof, "".into())),
        ArchiveError::Corrupt(_)
    ));
    // A real I/O fault stays Io.
    let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    assert!(matches!(map_sevenz_err(E::Io(denied, "".into())), ArchiveError::Io(_)));
}
