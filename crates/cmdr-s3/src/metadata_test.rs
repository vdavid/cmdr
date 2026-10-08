//! The mtime format, against the examples in `ncw/swift`'s `meta.go`.

use std::time::{Duration, UNIX_EPOCH};

use super::{format_mtime, parse_mtime};

#[test]
fn whole_seconds_have_no_decimal_point() {
    assert_eq!(
        format_mtime(UNIX_EPOCH + Duration::from_secs(1_354_040_105)),
        "1354040105"
    );
}

#[test]
fn nanoseconds_survive_and_trailing_zeros_drop() {
    assert_eq!(
        format_mtime(UNIX_EPOCH + Duration::new(1_354_040_105, 123_456_789)),
        "1354040105.123456789"
    );
    assert_eq!(
        format_mtime(UNIX_EPOCH + Duration::new(1_354_040_105, 500_000_000)),
        "1354040105.5"
    );
}

#[test]
fn under_a_second_keeps_a_leading_zero() {
    assert_eq!(format_mtime(UNIX_EPOCH + Duration::from_millis(5)), "0.005");
    assert_eq!(format_mtime(UNIX_EPOCH), "0");
}

#[test]
fn before_the_epoch_is_negative() {
    assert_eq!(format_mtime(UNIX_EPOCH - Duration::from_millis(1_500)), "-1.5");
    assert_eq!(parse_mtime("-1.5"), Some(UNIX_EPOCH - Duration::from_millis(1_500)));
}

#[test]
fn parsing_round_trips_and_pads_or_cuts_the_fraction_to_nanoseconds() {
    let at = UNIX_EPOCH + Duration::new(1_354_040_105, 123_456_789);
    assert_eq!(parse_mtime(&format_mtime(at)), Some(at));
    assert_eq!(
        parse_mtime("1354040105"),
        Some(UNIX_EPOCH + Duration::from_secs(1_354_040_105))
    );
    assert_eq!(parse_mtime("1.5"), Some(UNIX_EPOCH + Duration::from_millis(1_500)));
    assert_eq!(
        parse_mtime("1.1234567899"),
        Some(UNIX_EPOCH + Duration::new(1, 123_456_789))
    );
}

#[test]
fn garbage_is_not_an_mtime() {
    for text in ["", "abc", "1.2.3", "1e9", " 12", "1.x", "-"] {
        assert_eq!(parse_mtime(text), None, "{text:?}");
    }
}
