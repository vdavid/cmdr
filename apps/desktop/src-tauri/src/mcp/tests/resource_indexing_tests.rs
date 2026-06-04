//! Tests for the `cmdr://indexing` builder's pure formatting helpers.

use crate::mcp::resources::indexing::{format_duration_human, format_number};

#[test]
fn test_format_duration_human() {
    assert_eq!(format_duration_human(0), "0ms");
    assert_eq!(format_duration_human(500), "500ms");
    assert_eq!(format_duration_human(1_000), "1.0s");
    assert_eq!(format_duration_human(47_100), "47.1s");
    assert_eq!(format_duration_human(60_000), "1m");
    assert_eq!(format_duration_human(252_000), "4m 12s");
    assert_eq!(format_duration_human(3_600_000), "1h 00m");
    assert_eq!(format_duration_human(3_723_000), "1h 02m");
}

#[test]
fn test_format_number() {
    assert_eq!(format_number(0), "0");
    assert_eq!(format_number(999), "999");
    assert_eq!(format_number(1_000), "1,000");
    assert_eq!(format_number(142_301), "142,301");
    assert_eq!(format_number(1_000_000), "1,000,000");
}
