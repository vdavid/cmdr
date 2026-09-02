//! EXIF for an image row: what the camera wrote about the shot, never the pixels.
//!
//! `kamadak-exif` (`exif` in code) finds the EXIF block inside the container (JPEG,
//! TIFF, HEIF/HEIC, PNG, WebP; GIF and BMP carry none, so they are never opened for it)
//! and parses it; [`exif_facts`] is the pure shaper from its fields to the model-facing
//! [`ExifFacts`]. A container without a block, or with one that doesn't parse, is "no
//! EXIF": the image is still described, the `exif` key is simply absent.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{Exif, Field, In, Tag, Value};
use serde::Serialize;

/// The EXIF fields the model gets. Each is absent when the block doesn't carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifFacts {
    /// `DateTimeOriginal`, else `DateTime`, as the camera wrote it (`YYYY:MM:DD HH:MM:SS`,
    /// local to the camera). No time zone is invented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_taken: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    /// `LensModel`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    /// Spoken with its unit: "1/250 s".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_time: Option<String>,
    /// Spoken as photographers write it: "f/2.8".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub f_number: Option<String>,
    /// `PhotographicSensitivity`: "400".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<String>,
    /// Spoken with its unit: "50 mm".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<String>,
    /// Where the photo was taken. The sensitive item: a home address in two numbers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps: Option<GpsCoordinates>,
}

/// The EXIF orientation: the code cameras write, and what it means for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Orientation {
    /// 1–8, as in the EXIF spec.
    pub value: u32,
    /// The turn a viewer applies to show the stored pixels upright.
    pub spoken: &'static str,
}

/// Decimal degrees, WGS 84 as EXIF has it. North and east positive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpsCoordinates {
    pub latitude: Degrees,
    pub longitude: Degrees,
}

/// A finite coordinate in decimal degrees, rounded to six places (about 11 cm: what
/// the rationals carry, without float noise in the JSON). Serializes as a bare number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Degrees(f64);

/// Sound because [`Degrees::new`] is the one constructor and it rejects every
/// non-finite value: over finite floats `PartialEq` is already reflexive.
impl Eq for Degrees {}

impl Degrees {
    /// `None` for NaN or infinity (a zero denominator in the file), or past `limit`
    /// in either direction (90 for a latitude, 180 for a longitude).
    fn new(value: f64, limit: f64) -> Option<Self> {
        (value.is_finite() && value.abs() <= limit).then(|| Self((value * 1e6).round() / 1e6))
    }

    #[cfg(test)]
    pub(crate) fn get(self) -> f64 {
        self.0
    }
}

/// True when the classifier's MIME names a container `kamadak-exif` reads. GIF and BMP
/// carry no EXIF; skipping them here saves an open that could only say so.
pub(super) fn container_carries_exif(format: &str) -> bool {
    matches!(
        format,
        "image/jpeg" | "image/tiff" | "image/heic" | "image/png" | "image/webp"
    )
}

/// The EXIF facts of the image at `p`, when its container carries a block that parses
/// and says something. A separate open from the dimensions read: `kamadak-exif` needs a
/// seekable reader (HEIF boxes point across the file).
pub(super) fn read_exif(p: &Path) -> Option<ExifFacts> {
    let mut reader = BufReader::new(File::open(p).ok()?);
    let parsed = exif::Reader::new().read_from_container(&mut reader).ok()?;
    exif_facts(&parsed)
}

/// Shape the parsed block. Pure. `None` when none of the fields the model gets is
/// present (an empty struct would only cost tokens).
pub(super) fn exif_facts(parsed: &Exif) -> Option<ExifFacts> {
    let field = |tag| parsed.get_field(tag, In::PRIMARY);
    let facts = ExifFacts {
        date_taken: field(Tag::DateTimeOriginal)
            .and_then(date_string)
            .or_else(|| field(Tag::DateTime).and_then(date_string)),
        camera_make: field(Tag::Make).and_then(ascii_string),
        camera_model: field(Tag::Model).and_then(ascii_string),
        lens: field(Tag::LensModel).and_then(ascii_string),
        orientation: field(Tag::Orientation).and_then(orientation),
        exposure_time: field(Tag::ExposureTime).map(|f| spoken(parsed, f)),
        f_number: field(Tag::FNumber).map(|f| spoken(parsed, f)),
        iso: field(Tag::PhotographicSensitivity).map(|f| spoken(parsed, f)),
        focal_length: field(Tag::FocalLength).map(|f| spoken(parsed, f)),
        gps: gps(parsed),
    };
    let empty = facts.date_taken.is_none()
        && facts.camera_make.is_none()
        && facts.camera_model.is_none()
        && facts.lens.is_none()
        && facts.orientation.is_none()
        && facts.exposure_time.is_none()
        && facts.f_number.is_none()
        && facts.iso.is_none()
        && facts.focal_length.is_none()
        && facts.gps.is_none();
    (!empty).then_some(facts)
}

/// The value as `kamadak-exif` speaks it for the tag, with the tag's unit where it has
/// one: "1/250 s", "f/2.8", "400", "50 mm".
fn spoken(parsed: &Exif, field: &Field) -> String {
    field.display_value().with_unit(parsed).to_string()
}

/// An ASCII field's first string, trailing NULs and blanks dropped, lossy on the odd
/// camera that writes Latin-1 into it. `None` when empty. (`display_value` would quote
/// it and escape the non-ASCII bytes: a display form, not the value.)
fn ascii_string(field: &Field) -> Option<String> {
    let Value::Ascii(strings) = &field.value else {
        return None;
    };
    let bytes = strings.first()?;
    let end = bytes
        .iter()
        .rposition(|b| !matches!(b, 0 | b' ' | b'\t' | b'\r' | b'\n'))
        .map_or(0, |i| i + 1);
    let text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    (!text.is_empty()).then_some(text)
}

/// A date field as the camera wrote it, when it is a date: the spec's blank "unknown"
/// spelling and a malformed value are absent rather than relayed.
fn date_string(field: &Field) -> Option<String> {
    let Value::Ascii(strings) = &field.value else {
        return None;
    };
    let bytes = strings.first()?;
    exif::DateTime::from_ascii(bytes).ok()?;
    ascii_string(field)
}

/// The EXIF orientation table: the turn a viewer applies to show the stored pixels
/// upright (the spec's "row 0" / "column 0" wording, spoken).
fn orientation(field: &Field) -> Option<Orientation> {
    let value = field.value.get_uint(0)?;
    let spoken = match value {
        1 => "upright",
        2 => "mirrored horizontally",
        3 => "rotated 180°",
        4 => "mirrored vertically",
        5 => "mirrored horizontally and rotated 90° counterclockwise",
        6 => "rotated 90° clockwise",
        7 => "mirrored horizontally and rotated 90° clockwise",
        8 => "rotated 90° counterclockwise",
        _ => return None,
    };
    Some(Orientation { value, spoken })
}

/// Both coordinates as decimal degrees, or nothing: one alone is not a place. The
/// sign comes from the `Ref` field (`S` and `W` negative); a missing `Ref` reads as
/// north / east, as EXIF readers conventionally do.
fn gps(parsed: &Exif) -> Option<GpsCoordinates> {
    let latitude = coordinate(parsed, Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S', 90.0)?;
    let longitude = coordinate(parsed, Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W', 180.0)?;
    Some(GpsCoordinates { latitude, longitude })
}

/// One coordinate: degrees, minutes, seconds as three rationals, signed by `Ref`.
fn coordinate(parsed: &Exif, tag: Tag, ref_tag: Tag, negative_ref: u8, limit: f64) -> Option<Degrees> {
    let Value::Rational(dms) = &parsed.get_field(tag, In::PRIMARY)?.value else {
        return None;
    };
    let [degrees, minutes, seconds] = dms.get(..3)? else {
        return None;
    };
    let magnitude = degrees.to_f64() + minutes.to_f64() / 60.0 + seconds.to_f64() / 3600.0;
    let negative = parsed
        .get_field(ref_tag, In::PRIMARY)
        .and_then(ascii_string)
        .is_some_and(|r| {
            r.as_bytes()
                .first()
                .is_some_and(|b| b.eq_ignore_ascii_case(&negative_ref))
        });
    Degrees::new(if negative { -magnitude } else { magnitude }, limit)
}
