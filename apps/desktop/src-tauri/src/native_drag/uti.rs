//! Pure filename-extension → Uniform Type Identifier (UTI) mapping for drag-out
//! file promises.
//!
//! `NSFilePromiseProvider::initWithFileType:` takes a UTI string that tells the
//! destination (Finder) what kind of file the promise will produce, so it can
//! pick the right icon and behavior before a single byte streams. We derive it
//! from the filename extension with a small pure table — no new dependency, no
//! `UTType` framework call (which would need a main-thread hop and the file to
//! exist locally, neither of which holds for a virtual promise).
//!
//! Unknown extensions degrade to `public.data`, which Finder accepts for any
//! file. Directories use `public.folder` — Finder accepts a `public.folder`
//! promise and hands us a directory destination URL.

/// The fallback UTI for a file whose extension we don't recognize. Finder
/// accepts it for any file content.
pub const FALLBACK_FILE_UTI: &str = "public.data";

/// The UTI for a directory promise (the folder-drag path).
pub const FOLDER_UTI: &str = "public.folder";

/// Returns the UTI for a dragged item.
///
/// `is_directory` short-circuits to [`FOLDER_UTI`]. Otherwise the filename's
/// extension is mapped via [`uti_for_extension`] (fallback [`FALLBACK_FILE_UTI`]).
pub fn uti_for_item(file_name: &str, is_directory: bool) -> &'static str {
    if is_directory {
        return FOLDER_UTI;
    }
    let ext = file_name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    uti_for_extension(ext)
}

/// Maps a (case-insensitive) filename extension to a system UTI string.
///
/// Covers the common photo/video/audio/document/archive types a user drags out
/// of a phone or network share. Anything unrecognized → [`FALLBACK_FILE_UTI`].
pub fn uti_for_extension(ext: &str) -> &'static str {
    // Lowercase once for the match. An empty extension (no dot, or trailing
    // dot) falls straight through to the fallback.
    match ext.to_ascii_lowercase().as_str() {
        // Images
        "jpg" | "jpeg" | "jpe" => "public.jpeg",
        "png" => "public.png",
        "gif" => "com.compuserve.gif",
        "tiff" | "tif" => "public.tiff",
        "bmp" => "com.microsoft.bmp",
        "heic" | "heif" => "public.heic",
        "webp" => "org.webmproject.webp",
        "svg" => "public.svg-image",
        "raw" | "dng" => "public.camera-raw-image",
        // Video
        "mov" | "qt" => "com.apple.quicktime-movie",
        "mp4" | "m4v" => "public.mpeg-4",
        "avi" => "public.avi",
        "mkv" => "org.matroska.mkv",
        "webm" => "org.webmproject.webm",
        "m2ts" | "mts" => "public.avchd-collection",
        // Audio
        "mp3" => "public.mp3",
        "m4a" => "public.mpeg-4-audio",
        "aac" => "public.aac-audio",
        "wav" => "com.microsoft.waveform-audio",
        "aiff" | "aif" => "public.aiff-audio",
        "flac" => "org.xiph.flac",
        "ogg" | "oga" => "org.xiph.ogg-audio",
        // Documents
        "pdf" => "com.adobe.pdf",
        "txt" | "text" => "public.plain-text",
        "rtf" => "public.rtf",
        "html" | "htm" => "public.html",
        "csv" => "public.comma-separated-values-text",
        "json" => "public.json",
        "xml" => "public.xml",
        "doc" => "com.microsoft.word.doc",
        "docx" => "org.openxmlformats.wordprocessingml.document",
        "xls" => "com.microsoft.excel.xls",
        "xlsx" => "org.openxmlformats.spreadsheetml.sheet",
        "ppt" => "com.microsoft.powerpoint.ppt",
        "pptx" => "org.openxmlformats.presentationml.presentation",
        // Archives
        "zip" => "public.zip-archive",
        "gz" | "gzip" => "org.gnu.gnu-zip-archive",
        "tar" => "public.tar-archive",
        "7z" => "org.7-zip.7-zip-archive",
        "rar" => "com.rarlab.rar-archive",
        // Unknown → universal fallback.
        _ => FALLBACK_FILE_UTI,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_always_map_to_public_folder() {
        assert_eq!(uti_for_item("DCIM", true), FOLDER_UTI);
        // Even a "directory" with a file-looking name maps to folder.
        assert_eq!(uti_for_item("archive.zip", true), FOLDER_UTI);
    }

    #[test]
    fn common_image_extensions_map_to_their_uti() {
        assert_eq!(uti_for_item("sunset.jpg", false), "public.jpeg");
        assert_eq!(uti_for_item("sunset.JPG", false), "public.jpeg");
        assert_eq!(uti_for_item("diagram.png", false), "public.png");
        assert_eq!(uti_for_item("IMG_2031.heic", false), "public.heic");
    }

    #[test]
    fn common_video_and_audio_extensions_map() {
        assert_eq!(uti_for_item("clip.mov", false), "com.apple.quicktime-movie");
        assert_eq!(uti_for_item("clip.MP4", false), "public.mpeg-4");
        assert_eq!(uti_for_item("song.mp3", false), "public.mp3");
    }

    #[test]
    fn pdf_and_text_documents_map() {
        assert_eq!(uti_for_item("report.pdf", false), "com.adobe.pdf");
        assert_eq!(uti_for_item("notes.txt", false), "public.plain-text");
    }

    #[test]
    fn every_mapped_extension_resolves_to_its_uti() {
        // One row per match arm in `uti_for_extension`, including every alias.
        // Pins the whole table so deleting (or editing) any arm fails here
        // instead of silently degrading that type to the `public.data` fallback.
        let cases: &[(&str, &str)] = &[
            // Images
            ("jpg", "public.jpeg"),
            ("jpeg", "public.jpeg"),
            ("jpe", "public.jpeg"),
            ("png", "public.png"),
            ("gif", "com.compuserve.gif"),
            ("tiff", "public.tiff"),
            ("tif", "public.tiff"),
            ("bmp", "com.microsoft.bmp"),
            ("heic", "public.heic"),
            ("heif", "public.heic"),
            ("webp", "org.webmproject.webp"),
            ("svg", "public.svg-image"),
            ("raw", "public.camera-raw-image"),
            ("dng", "public.camera-raw-image"),
            // Video
            ("mov", "com.apple.quicktime-movie"),
            ("qt", "com.apple.quicktime-movie"),
            ("mp4", "public.mpeg-4"),
            ("m4v", "public.mpeg-4"),
            ("avi", "public.avi"),
            ("mkv", "org.matroska.mkv"),
            ("webm", "org.webmproject.webm"),
            ("m2ts", "public.avchd-collection"),
            ("mts", "public.avchd-collection"),
            // Audio
            ("mp3", "public.mp3"),
            ("m4a", "public.mpeg-4-audio"),
            ("aac", "public.aac-audio"),
            ("wav", "com.microsoft.waveform-audio"),
            ("aiff", "public.aiff-audio"),
            ("aif", "public.aiff-audio"),
            ("flac", "org.xiph.flac"),
            ("ogg", "org.xiph.ogg-audio"),
            ("oga", "org.xiph.ogg-audio"),
            // Documents
            ("pdf", "com.adobe.pdf"),
            ("txt", "public.plain-text"),
            ("text", "public.plain-text"),
            ("rtf", "public.rtf"),
            ("html", "public.html"),
            ("htm", "public.html"),
            ("csv", "public.comma-separated-values-text"),
            ("json", "public.json"),
            ("xml", "public.xml"),
            ("doc", "com.microsoft.word.doc"),
            ("docx", "org.openxmlformats.wordprocessingml.document"),
            ("xls", "com.microsoft.excel.xls"),
            ("xlsx", "org.openxmlformats.spreadsheetml.sheet"),
            ("ppt", "com.microsoft.powerpoint.ppt"),
            ("pptx", "org.openxmlformats.presentationml.presentation"),
            // Archives
            ("zip", "public.zip-archive"),
            ("gz", "org.gnu.gnu-zip-archive"),
            ("gzip", "org.gnu.gnu-zip-archive"),
            ("tar", "public.tar-archive"),
            ("7z", "org.7-zip.7-zip-archive"),
            ("rar", "com.rarlab.rar-archive"),
        ];

        for (ext, expected) in cases {
            // Direct extension mapping.
            assert_eq!(uti_for_extension(ext), *expected, "uti_for_extension({ext:?})");
            // And through the public entry point with a filename.
            let file_name = format!("file.{ext}");
            assert_eq!(
                uti_for_item(&file_name, false),
                *expected,
                "uti_for_item({file_name:?})"
            );
            // Case-insensitive: an uppercase extension maps the same.
            let upper = format!("file.{}", ext.to_ascii_uppercase());
            assert_eq!(uti_for_item(&upper, false), *expected, "uti_for_item({upper:?})");
        }
    }

    #[test]
    fn unknown_extension_falls_back_to_public_data() {
        assert_eq!(uti_for_item("firmware.bin", false), FALLBACK_FILE_UTI);
        assert_eq!(uti_for_item("weird.xyzzy", false), FALLBACK_FILE_UTI);
    }

    #[test]
    fn no_extension_falls_back_to_public_data() {
        assert_eq!(uti_for_item("README", false), FALLBACK_FILE_UTI);
        // A leading-dot dotfile has no real extension after the name.
        assert_eq!(uti_for_item("Makefile", false), FALLBACK_FILE_UTI);
    }

    #[test]
    fn trailing_dot_is_treated_as_empty_extension() {
        assert_eq!(uti_for_item("weird.", false), FALLBACK_FILE_UTI);
    }

    #[test]
    fn multi_dot_name_uses_the_last_segment() {
        assert_eq!(uti_for_item("archive.tar.gz", false), "org.gnu.gnu-zip-archive");
        assert_eq!(uti_for_item("photo.backup.jpg", false), "public.jpeg");
    }
}
