//! The two questions the loss note asks: which classes a pair puts at risk, and
//! which of those a source item actually had something worth keeping in.

use std::ffi::OsString;
use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::metadata::{CarriedMetadata, UserFlags, Xattr};

const SMB_LIKE: MetadataCaps = MetadataCaps {
    stores: MetadataClasses::CREATED,
    reports: MetadataClasses::CREATED,
    folder_stores: MetadataClasses::CREATED,
    folder_dates: true,
    staged_flags: UserFlags::NONE,
};

/// What a source reports and the destination can't store is at risk, and only
/// that: a class the destination keeps isn't, and a class the source can't
/// report is unknown, never at risk.
#[test]
fn at_risk_is_what_the_source_reports_and_the_destination_cant_store() {
    assert_eq!(
        MetadataCaps::NONE.file_classes_at_risk_from(MetadataCaps::ALL),
        MetadataClasses::ALL
    );
    assert_eq!(
        SMB_LIKE.file_classes_at_risk_from(MetadataCaps::ALL),
        MetadataClasses::MODE | MetadataClasses::FLAGS | MetadataClasses::XATTRS
    );
    assert_eq!(
        MetadataCaps::NONE.file_classes_at_risk_from(SMB_LIKE),
        MetadataClasses::CREATED
    );
}

/// Invariant 10's shape: a pair that loses nothing puts nothing at risk, so a
/// copy between them never asks the source anything.
#[test]
fn a_pair_that_keeps_everything_puts_nothing_at_risk() {
    assert!(
        MetadataCaps::ALL
            .file_classes_at_risk_from(MetadataCaps::ALL)
            .is_empty()
    );
    assert!(SMB_LIKE.file_classes_at_risk_from(SMB_LIKE).is_empty());
    assert!(
        MetadataCaps::ALL
            .file_classes_at_risk_from(MetadataCaps::NONE)
            .is_empty()
    );
}

fn at(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn xattr(name: &str) -> Xattr {
    Xattr {
        name: OsString::from(name),
        value: Vec::new(),
    }
}

/// Each class counts only when it holds something a person would miss: a birth
/// before the last edit, a mode a fresh file wouldn't get, any flag, and an
/// xattr other than the quarantine.
#[test]
fn worth_keeping_counts_only_what_a_person_would_miss() {
    let carried = CarriedMetadata {
        created: Some(at(1_000)),
        mode: Some(0o755),
        flags: UserFlags::HIDDEN,
        xattrs: vec![xattr("com.apple.metadata:_kMDItemUserTags")],
    };
    assert_eq!(carried.worth_keeping(Some(at(2_000)), 0o644), MetadataClasses::ALL);
}

#[test]
fn a_birth_equal_to_the_last_edit_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        created: Some(at(2_000)),
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(Some(at(2_000)), 0o644).is_empty());
    assert_eq!(carried.worth_keeping(None, 0o644), MetadataClasses::CREATED);
}

#[test]
fn the_default_mode_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        mode: Some(0o644),
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(None, 0o644).is_empty());
    assert_eq!(carried.worth_keeping(None, 0o600), MetadataClasses::MODE);
}

#[test]
fn a_quarantine_alone_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        xattrs: vec![xattr("com.apple.quarantine")],
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(None, 0o644).is_empty());
}

#[test]
fn nothing_carried_is_nothing_worth_keeping() {
    assert!(CarriedMetadata::default().worth_keeping(Some(at(1)), 0o644).is_empty());
}

/// What waits for the landing: nothing when the write kept every flag the file
/// has, and then the WHOLE set, so a destination that replaces its flags as one
/// word (SMB's attributes) doesn't drop the ones the write already put on.
#[test]
fn only_a_flag_the_write_couldnt_keep_sends_the_set_after_the_landing() {
    let with = |flags| CarriedMetadata {
        flags,
        created: Some(at(1)),
        ..CarriedMetadata::default()
    };
    assert!(with(UserFlags::NONE).after_landing(UserFlags::NONE).is_empty());
    assert!(with(UserFlags::HIDDEN).after_landing(UserFlags::HIDDEN).is_empty());
    let all_after = with(UserFlags::HIDDEN).after_landing(UserFlags::NONE);
    assert_eq!(all_after.flags, UserFlags::HIDDEN);
    assert!(all_after.created.is_none(), "only the flags wait for the landing");
    assert_eq!(
        with(UserFlags::HIDDEN | UserFlags::LOCKED)
            .after_landing(UserFlags::HIDDEN)
            .flags,
        UserFlags::HIDDEN | UserFlags::LOCKED
    );
}

/// A copy asks for the values its destination keeps and an outline of the rest
/// the source reports; a class the source can't report is in neither.
#[test]
fn a_copy_asks_values_for_what_it_keeps_and_an_outline_for_what_it_loses() {
    let ask = MetadataAsk::for_copy(MetadataCaps::ALL, SMB_LIKE);
    assert_eq!(ask.values, MetadataClasses::CREATED);
    assert_eq!(
        ask.outline,
        MetadataClasses::MODE | MetadataClasses::FLAGS | MetadataClasses::XATTRS
    );
    assert!(ask.mentions(MetadataClasses::XATTRS));

    let ask = MetadataAsk::for_copy(SMB_LIKE, MetadataCaps::ALL);
    assert_eq!(ask.values, MetadataClasses::CREATED);
    assert_eq!(ask.outline, MetadataClasses::NONE);
    assert!(!ask.mentions(MetadataClasses::XATTRS));
}
