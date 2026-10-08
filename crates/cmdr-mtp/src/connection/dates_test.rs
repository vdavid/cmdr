use std::time::{Duration, UNIX_EPOCH};

use jiff::tz::TimeZone;
use mtp_rs::UtcOffset;

use super::*;

fn at(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
    DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        offset: None,
    }
}

/// Stockholm's rules as a POSIX TZ string, so a cell reads the same on every
/// machine and needs no tzdb: CET (UTC+1), and CEST (UTC+2) from the last Sunday
/// of March at 02:00 to the last Sunday of October at 03:00.
fn stockholm() -> TimeZone {
    TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").expect("a valid POSIX TZ string")
}

fn in_utc(dt: DateTime) -> Option<u64> {
    convert_mtp_datetime_in(dt, &TimeZone::UTC)
}

// Regression: the listing used 30-day months and a rough leap-year count, so a
// phone's photo dated 2021-01-29 listed days off, and an upload's date couldn't
// round-trip.
#[test]
fn a_listed_date_is_the_exact_unix_second() {
    assert_eq!(in_utc(at(1970, 1, 1, 0, 0, 0)), Some(0));
    assert_eq!(in_utc(at(2021, 1, 29, 8, 30, 15)), Some(1_611_909_015));
    // Past February of a leap year, and the 2000 leap century.
    assert_eq!(in_utc(at(2024, 3, 1, 0, 0, 0)), Some(1_709_251_200));
    assert_eq!(in_utc(at(2000, 12, 31, 23, 59, 59)), Some(978_307_199));
}

#[test]
fn a_zoneless_date_reads_at_the_offset_its_own_day_had_in_the_macs_zone() {
    // Winter: CET, an hour east of UTC.
    assert_eq!(
        convert_mtp_datetime_in(at(2021, 1, 29, 8, 30, 15), &stockholm()),
        Some(1_611_905_415)
    );
    // Summer: CEST, two hours east, whatever the offset is on the day this runs.
    assert_eq!(
        convert_mtp_datetime_in(at(2024, 7, 6, 12, 0, 0), &stockholm()),
        Some(1_720_260_000)
    );
}

#[test]
fn a_zoneless_date_the_clocks_passed_twice_reads_as_the_earlier_instant() {
    // 2024-10-27 02:30 happened at 00:30 UTC (CEST) and again at 01:30 UTC (CET).
    assert_eq!(
        convert_mtp_datetime_in(at(2024, 10, 27, 2, 30, 0), &stockholm()),
        Some(1_729_989_000)
    );
}

#[test]
fn a_zoneless_date_the_clocks_skipped_shifts_forward_by_the_gap() {
    // 2024-03-31 02:30 never happened in Stockholm; it reads as 03:30 CEST.
    assert_eq!(
        convert_mtp_datetime_in(at(2024, 3, 31, 2, 30, 0), &stockholm()),
        Some(1_711_848_600)
    );
}

#[test]
fn a_listed_date_with_its_own_offset_lands_on_that_instant() {
    let two_hours_east = UtcOffset::from_minutes(120).expect("in range");
    let dated = at(2021, 1, 29, 10, 30, 15).with_offset(two_hours_east);
    assert_eq!(in_utc(dated), Some(1_611_909_015));
    // The Mac's zone has no say over a date that names its own offset.
    assert_eq!(convert_mtp_datetime_in(dated, &stockholm()), Some(1_611_909_015));
}

#[test]
fn a_date_before_1970_or_with_impossible_fields_lists_none() {
    assert_eq!(in_utc(at(1969, 12, 31, 23, 59, 59)), None);
    // New Year's night in Stockholm was still 1969 in UTC.
    assert_eq!(convert_mtp_datetime_in(at(1970, 1, 1, 0, 30, 0), &stockholm()), None);
    assert_eq!(in_utc(at(2021, 0, 0, 0, 0, 0)), None);
    assert_eq!(in_utc(at(2023, 2, 29, 0, 0, 0)), None);
}

#[test]
fn an_upload_sends_its_date_in_utc_and_it_round_trips() {
    let date = UNIX_EPOCH + Duration::from_millis(1_611_909_015_750);
    let sent = mtp_datetime_from_system_time(date).expect("a 2021 date fits a PTP DateTime");
    assert_eq!(
        sent,
        at(2021, 1, 29, 8, 30, 15).with_offset(UtcOffset::UTC),
        "whole seconds, marked UTC so the device writes a `Z`; the fraction drops"
    );
    assert_eq!(convert_mtp_datetime_in(sent, &stockholm()), Some(1_611_909_015));

    let leap_day = UNIX_EPOCH + Duration::from_secs(1_709_164_800); // 2024-02-29
    let sent = mtp_datetime_from_system_time(leap_day).expect("fits");
    assert_eq!(sent, at(2024, 2, 29, 0, 0, 0).with_offset(UtcOffset::UTC));
}

#[test]
fn a_date_a_ptp_datetime_cant_spell_sends_none() {
    assert_eq!(mtp_datetime_from_system_time(UNIX_EPOCH - Duration::from_secs(1)), None);
    // PTP's year is four digits.
    assert_eq!(
        mtp_datetime_from_system_time(UNIX_EPOCH + Duration::from_secs(253_402_300_800)), // 10000-01-01
        None
    );
}
