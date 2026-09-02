//! Tests for the EXIF of an image row: the pure shaper over an authored block, and the
//! container path on a hand-assembled JPEG (alone, without a block, with a broken one,
//! and inside a zip). HEIF isn't authored here: `exif::experimental::Writer` writes TIFF
//! blocks, not ISO-BMFF boxes, so the HEIC container rides `kamadak-exif`'s own tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

// `::exif` is the crate; a bare `exif` here is the sibling module the glob import brings in.
use ::exif::experimental::Writer;
use ::exif::{Exif, Field, In, Rational, Tag, Value};

use super::exif::exif_facts;
use super::tests::assert_text_only;
use super::*;
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::archive_extract::{EXTRACT_CAP_BYTES, extract_if_archive_inner_with};
use crate::test_support::TestDir;
use cmdr_archive::test_fixtures::{build_zip, stored};

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn rational(tag: Tag, pairs: &[(u32, u32)]) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Rational(pairs.iter().map(|&(num, denom)| Rational { num, denom }).collect()),
    }
}

fn short(tag: Tag, n: u16) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![n]),
    }
}

/// An EXIF block (a TIFF structure) holding `fields`, as a camera would embed it.
fn exif_blob(fields: &[Field]) -> Vec<u8> {
    let mut writer = Writer::new();
    for field in fields {
        writer.push_field(field);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    writer.write(&mut buf, false).unwrap();
    buf.into_inner()
}

fn parse(blob: Vec<u8>) -> Exif {
    ::exif::Reader::new().read_raw(blob).unwrap()
}

/// Every field the shaper reads, shot at 59° 20' 45.31" N, 18° 3' 55.2" E.
fn full_shot(lat_ref: &str, long_ref: &str) -> Vec<Field> {
    vec![
        ascii(Tag::Make, "Canon"),
        ascii(Tag::Model, "Canon EOS R6"),
        short(Tag::Orientation, 6),
        ascii(Tag::DateTime, "2026:08:30 10:00:00"),
        ascii(Tag::DateTimeOriginal, "2026:08:29 18:42:07"),
        ascii(Tag::LensModel, "RF50mm F1.8 STM"),
        rational(Tag::ExposureTime, &[(1, 250)]),
        rational(Tag::FNumber, &[(28, 10)]),
        short(Tag::PhotographicSensitivity, 400),
        rational(Tag::FocalLength, &[(50, 1)]),
        ascii(Tag::GPSLatitudeRef, lat_ref),
        rational(Tag::GPSLatitude, &[(59, 1), (20, 1), (4531, 100)]),
        ascii(Tag::GPSLongitudeRef, long_ref),
        rational(Tag::GPSLongitude, &[(18, 1), (3, 1), (552, 10)]),
    ]
}

/// SOI, one APP1 segment carrying `Exif\0\0` + the block, EOI. Enough for the magic
/// check, the classifier, and `kamadak-exif`; there are no pixels to have a size.
fn jpeg_with_app1(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let len = u16::try_from(payload.len() + 2).unwrap();
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn jpeg_with_exif(fields: &[Field]) -> Vec<u8> {
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(exif_blob(fields));
    jpeg_with_app1(&payload)
}

fn write_bytes(dir: &TestDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn inspect(path: &Path) -> FileRow {
    inspect_path(
        path.to_str().unwrap(),
        &TextAsk::Window(WindowOpts {
            start_line: 1,
            max_lines: 200,
        }),
        &AtomicBool::new(false),
    )
}

fn image_of(row: &FileRow) -> &ImageContent {
    match row {
        FileRow::Ok(file) => match &file.content {
            Content::Image(image) => image,
            other => panic!("expected an image row, got {other:?}"),
        },
        other => panic!("expected an ok row, got {other:?}"),
    }
}

// ── The shaper ────────────────────────────────────────────────────────────────

#[test]
fn every_field_is_shaped_and_spoken() {
    let facts = exif_facts(&parse(exif_blob(&full_shot("N", "E")))).expect("a full block has facts");
    assert_eq!(
        facts.date_taken.as_deref(),
        Some("2026:08:29 18:42:07"),
        "DateTimeOriginal wins over DateTime"
    );
    assert_eq!(facts.camera_make.as_deref(), Some("Canon"));
    assert_eq!(facts.camera_model.as_deref(), Some("Canon EOS R6"));
    assert_eq!(facts.lens.as_deref(), Some("RF50mm F1.8 STM"));
    assert_eq!(
        facts.orientation,
        Some(Orientation {
            value: 6,
            spoken: "rotated 90° clockwise",
        })
    );
    assert_eq!(facts.exposure_time.as_deref(), Some("1/250 s"));
    assert_eq!(facts.f_number.as_deref(), Some("f/2.8"));
    assert_eq!(facts.iso.as_deref(), Some("400"));
    assert_eq!(facts.focal_length.as_deref(), Some("50 mm"));
    let gps = facts.gps.expect("both coordinates were written");
    assert_eq!(gps.latitude.get(), 59.345919);
    assert_eq!(gps.longitude.get(), 18.065333);
}

#[test]
fn gps_south_and_west_are_negative() {
    let facts = exif_facts(&parse(exif_blob(&full_shot("S", "W")))).unwrap();
    let gps = facts.gps.unwrap();
    assert_eq!(gps.latitude.get(), -59.345919);
    assert_eq!(gps.longitude.get(), -18.065333);
}

#[test]
fn date_taken_falls_back_to_date_time_and_a_blank_date_is_absent() {
    let facts = exif_facts(&parse(exif_blob(&[ascii(Tag::DateTime, "2026:08:30 10:00:00")]))).unwrap();
    assert_eq!(facts.date_taken.as_deref(), Some("2026:08:30 10:00:00"));

    // The spec's "unknown" spelling: spaces where the digits go.
    let blank = exif_facts(&parse(exif_blob(&[
        ascii(Tag::DateTimeOriginal, "    :  :     :  :  "),
        ascii(Tag::Make, "Canon"),
    ])))
    .unwrap();
    assert_eq!(blank.date_taken, None);
}

#[test]
fn a_lone_coordinate_or_a_zero_denominator_gives_no_gps() {
    let lone = exif_facts(&parse(exif_blob(&[
        ascii(Tag::Make, "Canon"),
        ascii(Tag::GPSLatitudeRef, "N"),
        rational(Tag::GPSLatitude, &[(59, 1), (20, 1), (0, 1)]),
    ])))
    .unwrap();
    assert_eq!(lone.gps, None, "a latitude without a longitude is not a place");

    let broken = exif_facts(&parse(exif_blob(&[
        ascii(Tag::Make, "Canon"),
        ascii(Tag::GPSLatitudeRef, "N"),
        rational(Tag::GPSLatitude, &[(59, 1), (20, 0), (0, 1)]),
        ascii(Tag::GPSLongitudeRef, "E"),
        rational(Tag::GPSLongitude, &[(18, 1), (3, 1), (0, 1)]),
    ])))
    .unwrap();
    assert_eq!(broken.gps, None, "a zero denominator is not a coordinate");
}

#[test]
fn an_orientation_outside_the_table_is_absent() {
    let facts = exif_facts(&parse(exif_blob(&[
        ascii(Tag::Make, "Canon"),
        short(Tag::Orientation, 9),
    ])))
    .unwrap();
    assert_eq!(facts.orientation, None);
}

#[test]
fn a_block_with_none_of_the_fields_is_no_facts() {
    let facts = exif_facts(&parse(exif_blob(&[ascii(Tag::ImageDescription, "a picture")])));
    assert_eq!(facts, None, "an empty struct would only cost tokens");
}

// ── The container path ────────────────────────────────────────────────────────

#[test]
fn a_jpeg_with_an_exif_block_carries_it_and_the_row_is_text_only() {
    let dir = TestDir::new("inspect_exif_jpeg");
    let jpeg = write_bytes(&dir, "shot.jpg", &jpeg_with_exif(&full_shot("N", "E")));
    let row = inspect(&jpeg);
    let image = image_of(&row);
    assert_eq!(image.format, "image/jpeg");
    let facts = image.exif.as_ref().expect("the APP1 block is read");
    assert_eq!(facts.camera_model.as_deref(), Some("Canon EOS R6"));
    assert_eq!(facts.gps.as_ref().map(|g| g.latitude.get()), Some(59.345919));

    let json = serde_json::to_value(&row).unwrap();
    assert_text_only(&json, "row");
    assert_eq!(json["content"]["exif"]["gps"]["latitude"], 59.345919);
    assert_eq!(json["content"]["exif"]["orientation"]["value"], 6);
    assert!(json["content"]["exif"].get("lens").is_some());
    assert!(json["content"].get("width").is_none(), "no SOF, no dimensions, no key");
}

#[test]
fn a_jpeg_without_an_exif_block_has_no_exif_key() {
    let dir = TestDir::new("inspect_exif_jpeg_none");
    let jpeg = write_bytes(&dir, "plain.jpg", &[0xFF, 0xD8, 0xFF, 0xD9]);
    let row = inspect(&jpeg);
    assert_eq!(image_of(&row).exif, None);
    let json = serde_json::to_value(&row).unwrap();
    assert!(json["content"].get("exif").is_none());
}

#[test]
fn a_png_without_an_exif_block_has_no_exif_key() {
    let dir = TestDir::new("inspect_exif_png_none");
    let png = dir.join("p.png");
    image::RgbaImage::new(2, 3).save(&png).unwrap();
    let row = inspect(&png);
    let image = image_of(&row);
    assert_eq!((image.width, image.height), (Some(2), Some(3)));
    assert_eq!(image.exif, None);
}

#[test]
fn a_broken_or_truncated_exif_block_is_an_image_row_with_no_exif_key() {
    let dir = TestDir::new("inspect_exif_broken");
    // Declares an EXIF block, carries garbage where the TIFF header goes.
    let garbage = write_bytes(
        &dir,
        "garbage.jpg",
        &jpeg_with_app1(b"Exif\0\0\xDE\xAD\xBE\xEF\xDE\xAD\xBE\xEF"),
    );
    let row = inspect(&garbage);
    assert_eq!(image_of(&row).exif, None, "garbage is no EXIF, not an error");

    // Declares a segment longer than the file.
    let mut truncated_bytes = vec![0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF];
    truncated_bytes.extend_from_slice(b"Exif\0\0MM\0\x2a");
    let truncated = write_bytes(&dir, "truncated.jpg", &truncated_bytes);
    let row = inspect(&truncated);
    assert_eq!(image_of(&row).exif, None, "a cut-off block is no EXIF, not an error");
}

#[test]
fn an_image_inside_a_zip_carries_its_exif() {
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
    let dir = TestDir::new("inspect_exif_zip");
    let zip = write_bytes(
        &dir,
        "photos.zip",
        &build_zip(&[stored("shot.jpg", jpeg_with_exif(&full_shot("N", "E")))]),
    );
    let extract_dir = TestDir::new("inspect_exif_zip_extract");
    let extract = |requested: &Path, volume_id: &str| {
        extract_if_archive_inner_with(requested, volume_id, &extract_dir, EXTRACT_CAP_BYTES)
    };
    let row = inspect_path_with(
        zip.join("shot.jpg").to_str().unwrap(),
        &TextAsk::Window(WindowOpts {
            start_line: 1,
            max_lines: 200,
        }),
        &AtomicBool::new(false),
        &extract,
    );
    let image = image_of(&row);
    assert_eq!(image.format, "image/jpeg");
    assert_eq!(
        image.exif.as_ref().and_then(|f| f.camera_make.as_deref()),
        Some("Canon"),
        "the extracted temp goes through the same image branch"
    );
}
