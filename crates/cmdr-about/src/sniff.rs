//! What a file's bytes are, against what its extension claims.
//!
//! Magic bytes through `infer`, plus the two it lacks: a disk image's `koly`
//! trailer and an installer package's `xar!` header. A mismatch is reported only
//! when both sides are confident: bytes Cmdr recognizes AND an extension it has a
//! claim for. A bundle is a folder, so it's never sniffed.

use infer::MatcherType;
use serde::Serialize;

use crate::text::ItemText;

/// How much of a file's start the host reads for [`real_type`].
pub const HEAD_BYTES: usize = 64 * 1024;

/// How much of a file's end the host reads: a disk image's trailer is its last
/// 512 bytes.
pub const TAIL_BYTES: usize = 512;

/// What a file really is, and whether its extension agrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RealType {
    /// What the bytes are.
    pub sniffed: Sniffed,
    /// What the extension claims.
    pub claimed: ExtensionClaim,
    /// Whether the two agree.
    pub agreement: TypeAgreement,
}

/// What a file's bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Sniffed {
    /// The family of formats.
    pub kind: SniffedKind,
    /// The exact format, as its usual extension (`png`, `mach`, `dmg`). `None`:
    /// the bytes match no format Cmdr knows.
    pub format: Option<String>,
}

/// What a file's extension claims it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionClaim {
    /// The extension, without its dot. `None`: the name has none.
    pub extension: Option<ItemText>,
    /// The family that extension belongs to. `None`: Cmdr has no claim for it.
    pub kind: Option<SniffedKind>,
}

/// Whether a file's bytes agree with its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TypeAgreement {
    /// The bytes are what the extension says.
    Matches,
    /// The bytes are something else (a `.jpg` that's a program).
    Mismatch,
    /// One side isn't confident: unrecognized bytes, or an extension with no
    /// claim.
    CantTell,
}

/// A family of file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SniffedKind {
    /// A disk image (`dmg`).
    DiskImage,
    /// An installer package (`pkg`).
    InstallerPackage,
    /// A program for macOS.
    MachO,
    /// A program for Linux.
    Elf,
    /// A program or installer for Windows.
    WindowsProgram,
    /// Other compiled code (Java, WebAssembly, Android).
    OtherCode,
    /// A script with an interpreter line.
    Script,
    /// An archive.
    Archive,
    /// A PDF.
    Pdf,
    /// An image.
    Image,
    /// A video.
    Video,
    /// A sound file.
    Audio,
    /// A font.
    Font,
    /// An office document or an e-book.
    Document,
    /// A database.
    Database,
    /// Text, HTML included.
    Text,
    /// A format Cmdr recognizes and has no family for.
    Other,
    /// Bytes that match no format Cmdr knows.
    Unrecognized,
}

/// What `head` (a file's first bytes) and `tail` (its last, when the host read
/// them) are.
pub(crate) fn sniff(head: &[u8], tail: Option<&[u8]>) -> Sniffed {
    let known = |kind, format: &str| Sniffed {
        kind,
        format: Some(format.to_owned()),
    };
    // A disk image is told by its trailer, whatever its first bytes look like (they're
    // the image's own data: a partition map, or compressed blocks).
    if tail.is_some_and(has_disk_image_trailer) {
        return known(SniffedKind::DiskImage, "dmg");
    }
    if head.starts_with(b"xar!") {
        return known(SniffedKind::InstallerPackage, "xar");
    }
    match infer::get(head) {
        Some(found) => known(kind_of(found.matcher_type(), found.extension()), found.extension()),
        None => Sniffed {
            kind: SniffedKind::Unrecognized,
            format: None,
        },
    }
}

/// A UDIF disk image ends in a 512-byte trailer that starts with `koly`.
fn has_disk_image_trailer(tail: &[u8]) -> bool {
    tail.len() >= TAIL_BYTES && tail[tail.len() - TAIL_BYTES..].starts_with(b"koly")
}

fn kind_of(matcher: MatcherType, format: &str) -> SniffedKind {
    match (matcher, format) {
        (_, "mach") => SniffedKind::MachO,
        (_, "elf") => SniffedKind::Elf,
        (_, "exe" | "dll" | "msi") => SniffedKind::WindowsProgram,
        (_, "sh") => SniffedKind::Script,
        (_, "pdf") => SniffedKind::Pdf,
        (_, "sqlite") => SniffedKind::Database,
        (_, "rtf") => SniffedKind::Document,
        (_, "eot") => SniffedKind::Font,
        (MatcherType::App, "wasm" | "class" | "dex" | "dey" | "bc" | "obj") => SniffedKind::OtherCode,
        (MatcherType::Archive, "swf" | "ps" | "nes" | "crx" | "dcm") => SniffedKind::Other,
        (MatcherType::Archive, _) => SniffedKind::Archive,
        (MatcherType::Image, _) => SniffedKind::Image,
        (MatcherType::Video, _) => SniffedKind::Video,
        (MatcherType::Audio, _) => SniffedKind::Audio,
        (MatcherType::Font, _) => SniffedKind::Font,
        (MatcherType::Doc | MatcherType::Book, _) => SniffedKind::Document,
        (MatcherType::Text, _) => SniffedKind::Text,
        (MatcherType::App | MatcherType::Custom, _) => SniffedKind::Other,
    }
}

/// The family an extension claims, or `None` when Cmdr has no claim for it.
fn claimed_kind(extension: &str) -> Option<SniffedKind> {
    let kind = match extension {
        "dmg" => SniffedKind::DiskImage,
        "pkg" | "mpkg" => SniffedKind::InstallerPackage,
        "dylib" => SniffedKind::MachO,
        "so" => SniffedKind::Elf,
        "exe" | "dll" | "msi" => SniffedKind::WindowsProgram,
        "class" | "wasm" | "dex" => SniffedKind::OtherCode,
        "sh" | "bash" | "zsh" | "command" => SniffedKind::Script,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" | "cab" | "deb" | "rpm" | "jar"
        | "apk" | "ipa" | "war" | "cpio" => SniffedKind::Archive,
        "pdf" => SniffedKind::Pdf,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "heif" | "avif" | "bmp" | "tif" | "tiff" | "ico" | "psd"
        | "jxl" | "cr2" => SniffedKind::Image,
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "mpg" | "mpeg" | "flv" => SniffedKind::Video,
        "mp3" | "m4a" | "ogg" | "opus" | "flac" | "wav" | "aac" | "aiff" | "aif" | "amr" => SniffedKind::Audio,
        "ttf" | "otf" | "woff" | "woff2" => SniffedKind::Font,
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf" | "epub" | "mobi"
        | "pages" | "numbers" | "key" => SniffedKind::Document,
        "sqlite" | "sqlite3" => SniffedKind::Database,
        "txt" | "md" | "csv" | "json" | "xml" | "yml" | "yaml" | "toml" | "log" | "html" | "htm" | "css" | "js"
        | "ts" | "rs" | "go" | "py" | "rb" | "c" | "h" | "swift" | "java" | "svg" => SniffedKind::Text,
        _ => return None,
    };
    Some(kind)
}

/// Whether bytes of family `sniffed` are what an extension claiming `claimed`
/// promises. Looser than equality where one format legitimately wears another's
/// bytes.
fn agrees(claimed: SniffedKind, sniffed: SniffedKind) -> bool {
    use SniffedKind::{Archive, Audio, Document, Elf, MachO, Script, Text, Video};
    claimed == sniffed
        || matches!(
            (claimed, sniffed),
            // One container format carries both (`m4a` is MP4, `ogg` holds either).
            (Audio, Video) | (Video, Audio)
            // Modern office documents and e-books are zip archives.
            | (Document, Archive)
            // A text file may start with an interpreter line.
            | (Text, Script)
            // A shared library is a program of whichever OS built it.
            | (Elf, MachO) | (MachO, Elf)
        )
}

/// What a file is: its bytes against its `extension` (without the dot, any case;
/// `None` when the name has none).
pub fn real_type(head: &[u8], tail: Option<&[u8]>, extension: Option<&str>) -> RealType {
    let sniffed = sniff(head, tail);
    let folded = extension.map(str::to_ascii_lowercase);
    let claimed_kind = folded.as_deref().and_then(claimed_kind);
    let agreement = match claimed_kind {
        Some(claimed) if sniffed.kind != SniffedKind::Unrecognized => {
            if agrees(claimed, sniffed.kind) {
                TypeAgreement::Matches
            } else {
                TypeAgreement::Mismatch
            }
        }
        Some(_) | None => TypeAgreement::CantTell,
    };
    RealType {
        sniffed,
        claimed: ExtensionClaim {
            extension: extension.map(ItemText::new),
            kind: claimed_kind,
        },
        agreement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const ZIP: &[u8] = b"PK\x03\x04\x14\0\0\0\x08\0";
    const PDF: &[u8] = b"%PDF-1.7\n";
    const MACH_O_64: &[u8] = b"\xcf\xfa\xed\xfe\x0c\0\0\x01";
    const MACH_O_FAT: &[u8] = b"\xca\xfe\xba\xbe\0\0\0\x02";
    const JAVA_CLASS: &[u8] = b"\xca\xfe\xba\xbe\0\0\0\x41";
    const SHELL: &[u8] = b"#!/bin/sh\necho hi\n";

    /// An ELF header: the magic plus enough bytes to be a header at all.
    fn elf() -> Vec<u8> {
        let mut head = b"\x7fELF\x02\x01\x01\0".to_vec();
        head.resize(64, 0);
        head
    }

    fn kind(head: &[u8]) -> SniffedKind {
        sniff(head, None).kind
    }

    fn disk_image_tail() -> Vec<u8> {
        let mut tail = vec![0u8; 100];
        tail.extend_from_slice(b"koly");
        tail.resize(100 + TAIL_BYTES, 0);
        tail
    }

    #[test]
    fn common_formats_are_told_by_their_first_bytes() {
        assert_eq!(kind(PNG), SniffedKind::Image);
        assert_eq!(kind(ZIP), SniffedKind::Archive);
        assert_eq!(kind(PDF), SniffedKind::Pdf);
        assert_eq!(kind(&elf()), SniffedKind::Elf);
        assert_eq!(kind(SHELL), SniffedKind::Script);
        assert_eq!(kind(b"SQLite format 3\0"), SniffedKind::Database);
        assert_eq!(kind(b"just some words"), SniffedKind::Unrecognized);
        assert_eq!(kind(b""), SniffedKind::Unrecognized);
        assert_eq!(sniff(PNG, None).format.as_deref(), Some("png"));
        assert_eq!(sniff(b"just some words", None).format, None);
    }

    #[test]
    fn thin_and_fat_mach_o_are_programs_and_a_java_class_isnt_one() {
        assert_eq!(kind(MACH_O_64), SniffedKind::MachO);
        assert_eq!(kind(MACH_O_FAT), SniffedKind::MachO);
        assert_eq!(kind(JAVA_CLASS), SniffedKind::OtherCode);
    }

    #[test]
    fn an_installer_package_is_told_by_its_xar_header() {
        let sniffed = sniff(b"xar!\0\x1c\0\x01", None);
        assert_eq!(sniffed.kind, SniffedKind::InstallerPackage);
        assert_eq!(sniffed.format.as_deref(), Some("xar"));
    }

    #[test]
    fn a_disk_image_is_told_by_its_trailer_whatever_its_first_bytes_are() {
        let tail = disk_image_tail();
        assert_eq!(sniff(b"\0\0\0\0", Some(&tail)).kind, SniffedKind::DiskImage);
        // A compressed image's first bytes can look like another format.
        assert_eq!(sniff(ZIP, Some(&tail)).kind, SniffedKind::DiskImage);
        // `koly` anywhere but 512 bytes from the end is no trailer.
        let mut shifted = tail.clone();
        shifted.push(0);
        assert_eq!(sniff(b"\0\0\0\0", Some(&shifted)).kind, SniffedKind::Unrecognized);
        assert_eq!(sniff(b"\0\0\0\0", Some(b"koly")).kind, SniffedKind::Unrecognized);
        // Without the tail there's nothing to tell it by.
        assert_eq!(sniff(b"\0\0\0\0", None).kind, SniffedKind::Unrecognized);
    }

    #[test]
    fn a_program_behind_a_picture_name_is_a_mismatch() {
        let found = real_type(MACH_O_64, None, Some("JPG"));
        assert_eq!(found.agreement, TypeAgreement::Mismatch);
        assert_eq!(found.sniffed.kind, SniffedKind::MachO);
        assert_eq!(found.claimed.kind, Some(SniffedKind::Image));
        assert_eq!(found.claimed.extension, Some(ItemText::new("JPG")));

        assert_eq!(real_type(ZIP, None, Some("jpg")).agreement, TypeAgreement::Mismatch);
        assert_eq!(
            real_type(MACH_O_64, None, Some("txt")).agreement,
            TypeAgreement::Mismatch
        );
        assert_eq!(
            real_type(MACH_O_64, None, Some("command")).agreement,
            TypeAgreement::Mismatch
        );
        assert_eq!(real_type(PNG, None, Some("dmg")).agreement, TypeAgreement::Mismatch);
    }

    #[test]
    fn a_file_that_is_what_it_says_matches() {
        assert_eq!(real_type(PNG, None, Some("png")).agreement, TypeAgreement::Matches);
        assert_eq!(real_type(PDF, None, Some("pdf")).agreement, TypeAgreement::Matches);
        assert_eq!(
            real_type(b"\0", Some(&disk_image_tail()), Some("dmg")).agreement,
            TypeAgreement::Matches
        );
        // Formats that legitimately wear another family's bytes.
        assert_eq!(real_type(ZIP, None, Some("docx")).agreement, TypeAgreement::Matches);
        assert_eq!(real_type(ZIP, None, Some("pages")).agreement, TypeAgreement::Matches);
        assert_eq!(real_type(SHELL, None, Some("js")).agreement, TypeAgreement::Matches);
        assert_eq!(real_type(MACH_O_64, None, Some("so")).agreement, TypeAgreement::Matches);
    }

    #[test]
    fn a_mismatch_needs_both_sides_confident() {
        // Bytes Cmdr doesn't recognize: plain text has no magic.
        assert_eq!(
            real_type(b"hello", None, Some("jpg")).agreement,
            TypeAgreement::CantTell
        );
        // An extension with no claim.
        assert_eq!(real_type(PNG, None, Some("xyz")).agreement, TypeAgreement::CantTell);
        // No extension at all: a program with no extension is ordinary.
        let bare = real_type(MACH_O_64, None, None);
        assert_eq!(bare.agreement, TypeAgreement::CantTell);
        assert_eq!(bare.claimed.extension, None);
        // A disk image whose tail the host couldn't read.
        assert_eq!(real_type(b"\0\0", None, Some("dmg")).agreement, TypeAgreement::CantTell);
    }
}
