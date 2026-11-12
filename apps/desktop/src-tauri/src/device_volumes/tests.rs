//! The fold from provider entries to picker entries, and the path prefix rule,
//! against a fake provider so no device is needed.

use super::*;

/// A provider with a fixed entry list.
struct FakeProvider {
    id: &'static str,
    entries: Vec<DeviceVolumeEntry>,
}

impl DeviceVolumeProvider for FakeProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn entries(&self) -> ProviderFuture<'_, Vec<DeviceVolumeEntry>> {
        Box::pin(async { self.entries.clone() })
    }

    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> ProviderFuture<'a, bool> {
        Box::pin(async move { self.entries.iter().any(|e| e.id == volume_id) })
    }

    fn space_for_path<'a>(&'a self, _path: &'a str) -> ProviderFuture<'a, Option<(u64, u64)>> {
        Box::pin(async { None })
    }

    fn eject<'a>(&'a self, _volume_id: &'a str) -> ProviderFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

fn entry(id: &str, path: &str, fs_type: &'static str) -> DeviceVolumeEntry {
    DeviceVolumeEntry {
        id: id.to_string(),
        name: format!("Device {id}"),
        path: path.to_string(),
        fs_type,
        mount_is_read_only: false,
        device_readiness: Some(DeviceReadiness::Ready),
        usb_speed: None,
        device_serial: None,
    }
}

/// A provider's row for one phone: `name` over `fs_type`, with `serial` as its identity.
fn phone_entry(fs_type: &'static str, name: &str, serial: &str) -> DeviceVolumeEntry {
    DeviceVolumeEntry {
        name: name.to_string(),
        device_serial: Some(serial.to_string()),
        ..entry(
            &format!("{fs_type}-{serial}"),
            &format!("{fs_type}://{serial}"),
            fs_type,
        )
    }
}

/// The row names the listing ends up with for `mtp`'s and `adb`'s entries, in that order.
async fn names_after_fold(mtp: Vec<DeviceVolumeEntry>, adb: Vec<DeviceVolumeEntry>) -> Vec<String> {
    let providers: Vec<Arc<dyn DeviceVolumeProvider>> = vec![
        Arc::new(FakeProvider {
            id: "mtp",
            entries: mtp,
        }),
        Arc::new(FakeProvider {
            id: "adb",
            entries: adb,
        }),
    ];
    let mut volumes = Vec::new();
    append_from(&mut volumes, &providers).await;
    volumes.into_iter().map(|v| v.name).collect()
}

/// `adb devices -l` names no model for an `unauthorized` phone, so its row is the bare serial
/// (verified on a Pixel 9 Pro XL, adb 37.0.0, `adb devices -l`, 2026-10-30), and nothing then
/// says it's the same phone as the MTP row beside it.
#[tokio::test]
async fn a_serial_only_row_takes_the_name_of_the_phone_it_is() {
    let names = names_after_fold(
        vec![phone_entry("mtp", "Pixel 9 Pro XL", "TESTSERIAL0001")],
        vec![phone_entry("adb", "TESTSERIAL0001", "TESTSERIAL0001")],
    )
    .await;

    assert_eq!(names, ["Pixel 9 Pro XL", "Pixel 9 Pro XL"]);
}

/// The MTP row of a phone still connecting has to name its ADB twin too, or a phone waiting on both
/// shows up as two devices, one of them nameless.
#[tokio::test]
async fn a_serial_only_row_takes_the_name_of_a_phone_still_connecting() {
    let connecting = DeviceVolumeEntry {
        device_readiness: Some(DeviceReadiness::Connecting),
        ..phone_entry("mtp", "Pixel 9 Pro XL", "TESTSERIAL0001")
    };
    let names = names_after_fold(
        vec![connecting],
        vec![phone_entry("adb", "TESTSERIAL0001", "TESTSERIAL0001")],
    )
    .await;

    assert_eq!(names, ["Pixel 9 Pro XL", "Pixel 9 Pro XL"]);
}

#[tokio::test]
async fn a_serial_only_row_matches_whatever_case_each_side_reports_the_serial_in() {
    let names = names_after_fold(
        vec![phone_entry("mtp", "Pixel 9 Pro XL", "testserial0001")],
        vec![phone_entry("adb", "TESTSERIAL0001", "TESTSERIAL0001")],
    )
    .await;

    assert_eq!(names[1], "Pixel 9 Pro XL");
}

#[tokio::test]
async fn a_serial_only_row_keeps_its_serial_when_nothing_else_knows_the_phone() {
    let names = names_after_fold(
        vec![phone_entry("mtp", "Galaxy S24", "OTHER")],
        vec![phone_entry("adb", "TESTSERIAL0001", "TESTSERIAL0001")],
    )
    .await;

    assert_eq!(names[1], "TESTSERIAL0001");
}

#[tokio::test]
async fn a_row_that_already_has_a_name_keeps_it() {
    let names = names_after_fold(
        vec![phone_entry("mtp", "Pixel 9 Pro XL", "TESTSERIAL0001")],
        vec![phone_entry("adb", "Pixel 9 Pro", "TESTSERIAL0001")],
    )
    .await;

    assert_eq!(names[1], "Pixel 9 Pro");
}

#[tokio::test]
async fn append_from_folds_every_provider_into_mobile_device_entries() {
    let providers: Vec<Arc<dyn DeviceVolumeProvider>> = vec![
        Arc::new(FakeProvider {
            id: "mtp",
            entries: vec![entry("mtp-1:65537", "mtp://mtp-1/65537", "mtp")],
        }),
        Arc::new(FakeProvider {
            id: "adb",
            entries: vec![entry("adb-serial", "adb://serial", "adb")],
        }),
    ];

    let mut volumes = Vec::new();
    append_from(&mut volumes, &providers).await;

    assert_eq!(volumes.len(), 2);
    let ids: Vec<&str> = volumes.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(
        ids,
        ["mtp-1:65537", "adb-serial"],
        "registration order is listing order"
    );
    for v in &volumes {
        assert!(matches!(v.category, LocationCategory::MobileDevice));
        assert!(v.is_ejectable);
        assert!(!v.supports_trash);
        assert!(v.capabilities.is_none(), "enrichment fills capabilities afterwards");
        assert!(v.icon.is_none());
        assert!(!v.is_disk_image);
        assert!(v.connection_state.is_none());
    }
    assert_eq!(volumes[0].fs_type.as_deref(), Some("mtp"));
    assert_eq!(volumes[1].fs_type.as_deref(), Some("adb"));
}
