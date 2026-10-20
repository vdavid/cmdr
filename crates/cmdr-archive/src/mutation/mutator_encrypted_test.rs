//! Encrypted entries through an edit (#592). A kept encrypted entry is raw
//! copied: its ciphertext (with ZipCrypto's 12-byte header, or AES's salt,
//! verifier, and MAC), its encryption flag, its data-descriptor flag, its CRC,
//! its DOS time, and its AES extra field all come through as stored, so it
//! still decrypts with the original password. Each fact is pinned here, for
//! ZipCrypto in both shapes (the CRC check byte, and Info-ZIP's bit 3 with the
//! time check byte) and for WinZip AES. An encrypted entry whose scheme Cmdr
//! can't carry faithfully still refuses the edit, the original untouched.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::test_fixtures::{
    CryptoFixtureFile, build_aes_zip, build_zipcrypto_zip, build_zipcrypto_zip_streamed, encrypted_entry, plain_entry,
};

const PASSWORD: &str = "hunter2";
const SECRET: &[u8] = b"classified payload";
const SECRET_NAME: &str = "secret.txt";

/// General-purpose bits a raw copy must keep as stored: encrypted (0), data
/// descriptor (3, which also picks ZipCrypto's check byte), strong encryption (6).
const CARRIED_FLAGS: u16 = 0b0100_1001;

#[derive(Debug, Clone, Copy)]
enum Scheme {
    /// ZipCrypto, check byte = the CRC's high byte, no data descriptor.
    ZipCrypto,
    /// ZipCrypto as Info-ZIP's `zip -e` writes it: bit 3, a data descriptor,
    /// and the check byte = the DOS time's high byte.
    ZipCryptoStreamed,
    Aes128,
    Aes256,
}

const SCHEMES: [Scheme; 4] = [
    Scheme::ZipCrypto,
    Scheme::ZipCryptoStreamed,
    Scheme::Aes128,
    Scheme::Aes256,
];

impl Scheme {
    fn is_zipcrypto(self) -> bool {
        matches!(self, Scheme::ZipCrypto | Scheme::ZipCryptoStreamed)
    }
}

fn entries() -> Vec<CryptoFixtureFile> {
    vec![
        encrypted_entry(SECRET_NAME, SECRET),
        plain_entry("plain.txt", "plain"),
        plain_entry("docs/readme.txt", "read me"),
    ]
}

fn fixture(scheme: Scheme) -> Vec<u8> {
    match scheme {
        Scheme::ZipCrypto => build_zipcrypto_zip(&entries(), PASSWORD),
        Scheme::ZipCryptoStreamed => build_zipcrypto_zip_streamed(&entries(), PASSWORD),
        Scheme::Aes128 => build_aes_zip(&entries(), PASSWORD, ::zip::AesMode::Aes128),
        Scheme::Aes256 => build_aes_zip(&entries(), PASSWORD, ::zip::AesMode::Aes256),
    }
}

fn write_zip(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("a.zip");
    std::fs::write(&path, bytes).expect("write the fixture zip");
    path
}

fn add_bytes(inner_path: &str, content: &[u8]) -> AddEntry {
    AddEntry {
        inner_path: inner_path.to_string(),
        source: AddSource::Bytes(content.to_vec()),
    }
}

/// Edits that leave the encrypted entry alone, each with a label for failures.
fn edits_beside_the_encrypted_entry() -> Vec<(&'static str, Changeset)> {
    vec![
        (
            "rename a plain entry",
            Changeset {
                renames: vec![("plain.txt".into(), "renamed.txt".into())],
                ..Default::default()
            },
        ),
        (
            "delete a plain entry",
            Changeset {
                deletes: vec!["plain.txt".into()],
                ..Default::default()
            },
        ),
        (
            "add an entry",
            Changeset {
                adds: vec![add_bytes("new.txt", b"fresh")],
                ..Default::default()
            },
        ),
        (
            "rename a folder",
            Changeset {
                renames: vec![("docs".into(), "manuals".into())],
                ..Default::default()
            },
        ),
    ]
}

/// Everything about a stored entry a reader needs to decrypt it.
#[derive(Debug, PartialEq, Eq)]
struct StoredFacts {
    /// The stored bytes: ciphertext plus the encryption header (or AES framing).
    raw: Vec<u8>,
    /// The carried general-purpose bits, from the central directory.
    central_flags: u16,
    /// The same bits, from the local header.
    local_flags: u16,
    crc32: u32,
    compressed_size: u64,
    size: u64,
    method: CompressionMethod,
    dos_time: Option<(u16, u16)>,
    /// The AES mode and vendor version, as `Debug` (the `zip` crate doesn't
    /// export the version's type).
    aes: Option<String>,
}

fn stored_facts(path: &Path, name: &str) -> StoredFacts {
    use ::zip::read::HasZipMetadata as _;
    let bytes = std::fs::read(path).expect("read the zip");
    let mut archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    let index = archive.index_for_name(name).expect("the entry is there");
    let mut file = archive.by_index_raw(index).expect("open the entry raw");
    let header = usize::try_from(file.header_start()).expect("an offset that fits");
    let local_flags = u16::from_le_bytes([bytes[header + 6], bytes[header + 7]]) & CARRIED_FLAGS;
    let mut facts = StoredFacts {
        raw: Vec::new(),
        central_flags: file.flags().as_u16() & CARRIED_FLAGS,
        local_flags,
        crc32: file.crc32(),
        compressed_size: file.compressed_size(),
        size: file.size(),
        method: file.compression(),
        dos_time: file.last_modified().map(|time| (time.datepart(), time.timepart())),
        aes: file
            .get_metadata()
            .aes_settings()
            .map(|settings| format!("{settings:?}")),
    };
    file.read_to_end(&mut facts.raw).expect("read the stored bytes");
    facts
}

fn decrypted(path: &Path, name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    let mut file = archive
        .by_name_decrypt(name, PASSWORD.as_bytes())
        .unwrap_or_else(|e| panic!("decrypting {name} with the original password: {e}"));
    let mut out = Vec::new();
    file.read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("reading {name} back (CRC/MAC intact): {e}"));
    out
}

/// How many times WinZip AES's extra field (id `0x9901`, 7 data bytes) appears
/// in the archive: once per encrypted entry per header, never twice.
fn aes_extra_field_count(path: &Path) -> usize {
    let bytes = std::fs::read(path).expect("read the zip");
    bytes.windows(4).filter(|w| *w == [0x01, 0x99, 0x07, 0x00]).count()
}

/// `unzip -P <password> -t` on the archive: an independent reader checking the
/// ZipCrypto check byte and the CRC. `None` when `unzip` isn't installed.
fn unzip_decrypts(path: &Path) -> Option<bool> {
    let output = Command::new("unzip")
        .args(["-P", PASSWORD, "-t"])
        .arg(path)
        .output()
        .ok()?;
    Some(output.status.success())
}

fn assert_encrypted_entry_intact(scheme: Scheme, label: &str, path: &Path, before: &StoredFacts, aes_fields: usize) {
    let after = stored_facts(path, SECRET_NAME);
    assert_eq!(
        &after, before,
        "{scheme:?}, {label}: the encrypted entry's stored facts"
    );
    assert!(
        after.central_flags & 1 == 1,
        "{scheme:?}, {label}: still flagged encrypted"
    );
    assert_eq!(decrypted(path, SECRET_NAME), SECRET, "{scheme:?}, {label}: plaintext");
    assert_eq!(
        aes_extra_field_count(path),
        aes_fields,
        "{scheme:?}, {label}: one AES extra field per header, as before"
    );
    if scheme.is_zipcrypto() {
        assert_ne!(
            unzip_decrypts(path),
            Some(false),
            "{scheme:?}, {label}: `unzip -P` decrypts the result"
        );
    }
}

#[test]
fn the_fixtures_decrypt_before_any_edit() {
    // Guards the fixtures themselves, so a failure below is the edit's.
    for scheme in SCHEMES {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = write_zip(tmp.path(), &fixture(scheme));
        assert_eq!(decrypted(&path, SECRET_NAME), SECRET, "{scheme:?}");
        if scheme.is_zipcrypto() {
            assert_ne!(
                unzip_decrypts(&path),
                Some(false),
                "{scheme:?}: `unzip -P` decrypts the fixture"
            );
        }
    }
}

#[test]
fn an_edit_beside_an_encrypted_entry_keeps_it_byte_identical_and_decryptable() {
    for scheme in SCHEMES {
        for (label, changeset) in edits_beside_the_encrypted_entry() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let path = write_zip(tmp.path(), &fixture(scheme));
            let before = stored_facts(&path, SECRET_NAME);
            let aes_fields = aes_extra_field_count(&path);

            apply(&path, &changeset, &NoHooks).unwrap_or_else(|e| panic!("{scheme:?}, {label}: the edit applies: {e}"));

            assert_encrypted_entry_intact(scheme, label, &path, &before, aes_fields);
        }
    }
}

#[test]
fn the_edit_itself_lands_beside_the_encrypted_entry() {
    for scheme in SCHEMES {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = write_zip(tmp.path(), &fixture(scheme));
        let changeset = Changeset {
            renames: vec![("plain.txt".into(), "renamed.txt".into())],
            adds: vec![add_bytes("new.txt", b"fresh")],
            deletes: vec!["docs".into()],
            ..Default::default()
        };
        apply(&path, &changeset, &NoHooks).expect("the edit applies");

        let mut archive = ZipArchive::new(File::open(&path).expect("open")).expect("parses");
        let mut names: Vec<String> = archive.file_names().map(|n| n.expect("a name").into_owned()).collect();
        names.sort();
        assert_eq!(names, ["new.txt", "renamed.txt", "secret.txt"], "{scheme:?}");
        let mut renamed = String::new();
        archive
            .by_name("renamed.txt")
            .expect("the renamed entry")
            .read_to_string(&mut renamed)
            .expect("read it");
        assert_eq!(renamed, "plain", "{scheme:?}");
    }
}

#[test]
fn renaming_the_encrypted_entry_keeps_it_decryptable() {
    // A rename touches only the name, which neither ZipCrypto nor AES binds to
    // the ciphertext, so it raw-copies like any kept entry.
    for scheme in SCHEMES {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = write_zip(tmp.path(), &fixture(scheme));
        let before = stored_facts(&path, SECRET_NAME);
        let changeset = Changeset {
            renames: vec![(SECRET_NAME.into(), "vault/moved.txt".into())],
            ..Default::default()
        };
        apply(&path, &changeset, &NoHooks).unwrap_or_else(|e| panic!("{scheme:?}: the rename applies: {e}"));

        assert_eq!(stored_facts(&path, "vault/moved.txt"), before, "{scheme:?}");
        assert_eq!(decrypted(&path, "vault/moved.txt"), SECRET, "{scheme:?}");
        if scheme.is_zipcrypto() {
            assert_ne!(
                unzip_decrypts(&path),
                Some(false),
                "{scheme:?}: `unzip -P` decrypts the result"
            );
        }
    }
}

#[test]
fn a_zip_from_info_zip_keeps_its_encrypted_entry_through_an_edit() {
    // A real `zip -e` archive (deflated, bit 3, the time check byte), where
    // the tool is installed. The hand-built fixtures above cover the shape
    // everywhere else.
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join(SECRET_NAME), SECRET.repeat(40)).expect("write the secret");
    std::fs::write(tmp.path().join("plain.txt"), b"plain").expect("write the plain file");
    let zip = |args: &[&str]| Command::new("zip").current_dir(tmp.path()).args(args).status();
    match zip(&["-q", "-P", PASSWORD, "a.zip", SECRET_NAME]) {
        Ok(status) if status.success() => {}
        _ => return, // `zip` isn't installed (a minimal CI image).
    }
    assert!(zip(&["-q", "a.zip", "plain.txt"]).expect("run zip").success());
    let path = tmp.path().join("a.zip");
    let before = stored_facts(&path, SECRET_NAME);
    assert_eq!(before.central_flags & 0b1001, 0b1001, "Info-ZIP sets bits 0 and 3");

    let changeset = Changeset {
        renames: vec![("plain.txt".into(), "renamed.txt".into())],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("the edit applies");

    assert_eq!(stored_facts(&path, SECRET_NAME), before);
    assert_eq!(decrypted(&path, SECRET_NAME), SECRET.repeat(40));
    assert_ne!(unzip_decrypts(&path), Some(false), "`unzip -P` decrypts the result");
}

// ---- What still refuses ---------------------------------------------------------

/// Sets general-purpose `bits` on the first entry, in its local header and its
/// central-directory record alike.
fn set_first_entry_flags(bytes: &mut [u8], bits: u16) {
    let central = bytes
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .expect("a central-directory record");
    for at in [6, central + 8] {
        let flags = u16::from_le_bytes([bytes[at], bytes[at + 1]]) | bits;
        bytes[at..at + 2].copy_from_slice(&flags.to_le_bytes());
    }
}

/// Sets the first entry's DOS date, in both headers, to `date`.
fn set_first_entry_dos_date(bytes: &mut [u8], date: u16) {
    bytes[12..14].copy_from_slice(&date.to_le_bytes());
    let central = bytes
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .expect("a central-directory record");
    bytes[central + 14..central + 16].copy_from_slice(&date.to_le_bytes());
}

fn assert_refused_untouched(bytes: &[u8], what: &str) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), bytes);
    let changeset = Changeset {
        adds: vec![add_bytes("note.txt", b"note")],
        ..Default::default()
    };
    let result = apply(&path, &changeset, &NoHooks);
    assert!(
        matches!(&result, Err(MutationError::EncryptedEntryRetained { name }) if name == SECRET_NAME),
        "{what}: got {result:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("re-read"),
        bytes,
        "{what}: original untouched"
    );
    let leftovers = std::fs::read_dir(tmp.path())
        .expect("read dir")
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"));
    assert!(!leftovers, "{what}: a refused edit creates no temp");

    // Deleting the entry it can't carry keeps nothing to carry.
    let changeset = Changeset {
        deletes: vec![SECRET_NAME.into()],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).unwrap_or_else(|e| panic!("{what}: deleting it applies: {e}"));
    let archive = ZipArchive::new(File::open(&path).expect("open")).expect("parses");
    assert!(archive.index_for_name(SECRET_NAME).is_none(), "{what}: deleted");
}

#[test]
fn pkware_strong_encryption_still_refuses() {
    // Bit 6: PKWARE's Strong Encryption, whose decryption header the `zip`
    // crate doesn't model, so nothing vouches that a copy keeps it readable.
    let mut bytes = build_zipcrypto_zip(&entries(), PASSWORD);
    set_first_entry_flags(&mut bytes, 1 << 6);
    assert_refused_untouched(&bytes, "strong encryption");
}

#[test]
fn a_time_checked_zipcrypto_entry_with_an_unreadable_date_still_refuses() {
    // Under bit 3 the password check byte is the DOS time's high byte, and the
    // `zip` crate drops a time it can't parse (date 0 has month 0), so a copy
    // couldn't write the stored one back.
    let mut bytes = build_zipcrypto_zip_streamed(&entries(), PASSWORD);
    set_first_entry_dos_date(&mut bytes, 0);
    assert_refused_untouched(&bytes, "unreadable date");
}
