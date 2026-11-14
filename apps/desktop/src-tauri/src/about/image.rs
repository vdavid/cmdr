//! The "What it shows" row: what Cmdr's image index already stored about a
//! picture. It reads `media.db` alone (`MediaIndex::facts_for_paths`, one point
//! lookup), ❌ never the picture: no decode, no Vision call, no file read, so a
//! picture on a sleeping NAS answers in milliseconds and wakes nothing.
//!
//! The same exclusions as `image_facts`: a folder the person left out of image
//! search answers as not indexed (the read API hides it), a path the sensitive
//! gate protects answers `Protected` before any lookup, and recognized text that
//! holds a private key does too.

use std::path::PathBuf;

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::ImageContents;
use cmdr_index::media_index::read::{ImageFacts, MediaIndex};

use crate::tools::sensitive::{classify, refuse_private_key_text};

/// Where a picture's index row may be.
#[derive(Debug, Clone)]
pub(crate) struct ImagePlace {
    /// The app's data dir, where each volume's `media.db` lives.
    pub(crate) data_dir: PathBuf,
    /// The volumes whose image index may hold it, first match wins: the item's
    /// own, then the drive index's (the boot disk's index walks a cloud drive).
    pub(crate) volume_ids: Vec<String>,
    /// The picture's path, as the index stored it.
    pub(crate) path: String,
}

impl ImagePlace {
    /// `volume_id`, then `index_id` when it's another volume.
    pub(crate) fn new(data_dir: PathBuf, volume_id: &str, index_id: &str, path: &str) -> Self {
        let mut volume_ids = vec![volume_id.to_string()];
        if index_id != volume_id {
            volume_ids.push(index_id.to_string());
        }
        Self {
            data_dir,
            volume_ids,
            path: path.to_string(),
        }
    }
}

/// The row's answer. `indexing_on`: image indexing's switch, read by the caller
/// (`media_index::gate::is_enabled`) so a test needn't flip the process's own.
pub(crate) fn image_contents_fact(place: &ImagePlace, indexing_on: bool) -> Fact<ImageContents> {
    if classify(&place.path).is_some() {
        return Fact::unknown(UnknownWhy::Protected);
    }
    if !indexing_on {
        return Fact::unknown(UnknownWhy::IndexingOff);
    }
    let mut unreadable = false;
    for volume_id in &place.volume_ids {
        let found = MediaIndex::open(&place.data_dir, volume_id).facts_for_paths(&[place.path.as_str()]);
        match found.map(|facts| facts.into_iter().next()) {
            Ok(Some(facts)) if facts.indexed => return fact_of(&facts),
            Ok(_) => {}
            Err(_) => unreadable = true,
        }
    }
    if unreadable {
        // The kind of failure, ❌ never the path: logs ride error reports.
        log::warn!(target: "about", "couldn't read the image index for a card");
        return Fact::unknown(UnknownWhy::Unparseable);
    }
    Fact::unknown(UnknownWhy::NotIndexed)
}

/// An indexed row as the card's fact. Text holding a private key (a screenshot of
/// one) answers `Protected`, judged on the WHOLE stored text before any cut.
fn fact_of(facts: &ImageFacts) -> Fact<ImageContents> {
    if let Some(Err(_)) = facts.ocr_text.as_deref().map(refuse_private_key_text) {
        return Fact::unknown(UnknownWhy::Protected);
    }
    let tags: Vec<(&str, f32)> = facts.tags.iter().map(|tag| (tag.label.as_str(), tag.score)).collect();
    Fact::Known(ImageContents::of(&tags, facts.ocr_text.as_deref()))
}

#[cfg(test)]
mod tests {
    use cmdr_about::text::ItemText;
    use cmdr_index::media_index::testing::seed_ocr_row;

    use super::*;
    use crate::test_support::TestDir;

    fn place(dir: &TestDir, path: &str) -> ImagePlace {
        ImagePlace::new(dir.as_ref().to_path_buf(), "root", "root", path)
    }

    #[test]
    fn an_indexed_picture_shows_its_recognized_text() {
        let dir = TestDir::new("about-image-indexed");
        let path = "/Users/someone/Pictures/receipt.png";
        seed_ocr_row(&dir, "root", path, "Corner Cafe\nLatte 45.00\nTotal 45.00\nThanks");

        let Fact::Known(contents) = image_contents_fact(&place(&dir, path), true) else {
            panic!("an indexed picture is known");
        };
        let lines: Vec<&str> = contents.text_lines.iter().map(ItemText::as_str).collect();
        assert_eq!(lines, ["Corner Cafe", "Latte 45.00", "Total 45.00"]);
        assert!(contents.more_text);
    }

    #[test]
    fn a_picture_the_index_never_reached_is_not_indexed_and_indexing_off_says_so() {
        let dir = TestDir::new("about-image-missing");
        let path = "/Users/someone/Pictures/new.png";
        assert_eq!(
            image_contents_fact(&place(&dir, path), true),
            Fact::unknown(UnknownWhy::NotIndexed)
        );
        seed_ocr_row(&dir, "root", path, "words");
        assert_eq!(
            image_contents_fact(&place(&dir, path), false),
            Fact::unknown(UnknownWhy::IndexingOff),
            "with indexing off, the row says so, whatever the store still holds"
        );
    }

    /// The second volume answers when the item's own has no row: the boot disk's
    /// index walks a cloud drive under another id.
    #[test]
    fn the_index_volume_answers_after_the_items_own() {
        let dir = TestDir::new("about-image-index-volume");
        let path = "/Users/someone/Library/CloudStorage/Drive/scan.png";
        seed_ocr_row(&dir, "root", path, "Lease agreement");
        let place = ImagePlace::new(dir.as_ref().to_path_buf(), "cloud-drive", "root", path);
        assert!(matches!(image_contents_fact(&place, true), Fact::Known(_)));
    }

    #[test]
    fn a_protected_path_is_never_looked_up() {
        let dir = TestDir::new("about-image-protected");
        let path = "/Users/someone/.ssh/key-photo.png";
        seed_ocr_row(&dir, "root", path, "secret words");
        assert_eq!(
            image_contents_fact(&place(&dir, path), true),
            Fact::unknown(UnknownWhy::Protected)
        );
    }

    /// OCR of a photographed key: no path rule sees it, the text check does, over the
    /// whole stored text.
    #[test]
    fn recognized_text_holding_a_private_key_is_protected() {
        let dir = TestDir::new("about-image-key");
        let path = "/Users/someone/Pictures/shot.png";
        let text = format!(
            "{}\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaA==",
            "line\n".repeat(50)
        );
        seed_ocr_row(&dir, "root", path, &text);
        assert_eq!(
            image_contents_fact(&place(&dir, path), true),
            Fact::unknown(UnknownWhy::Protected)
        );
    }
}
