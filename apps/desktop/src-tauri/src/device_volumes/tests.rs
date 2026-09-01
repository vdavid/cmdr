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

    fn entries(&self) -> Pin<Box<dyn Future<Output = Vec<DeviceVolumeEntry>> + Send + '_>> {
        Box::pin(async { self.entries.clone() })
    }

    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.entries.iter().any(|e| e.id == volume_id) })
    }

    fn space_for_path<'a>(&'a self, _path: &'a str) -> Pin<Box<dyn Future<Output = Option<(u64, u64)>> + Send + 'a>> {
        Box::pin(async { None })
    }

    fn eject<'a>(&'a self, _volume_id: &'a str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
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
        usb_speed: None,
    }
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
    assert_eq!(ids, ["mtp-1:65537", "adb-serial"], "registration order is listing order");
    for v in &volumes {
        assert!(matches!(v.category, LocationCategory::MobileDevice));
        assert!(v.is_ejectable);
        assert!(!v.supports_trash);
        assert!(v.capabilities.is_none(), "enrichment fills capabilities afterwards");
        assert!(v.icon.is_none());
        assert!(!v.is_disk_image);
        assert!(v.smb_connection_state.is_none());
    }
    assert_eq!(volumes[0].fs_type.as_deref(), Some("mtp"));
    assert_eq!(volumes[1].fs_type.as_deref(), Some("adb"));
}

#[test]
fn a_path_is_under_its_root_or_a_slash_separated_child_of_it() {
    assert!(path_is_under("adb://serial", "adb://serial"));
    assert!(path_is_under("adb://serial/sdcard/DCIM", "adb://serial"));
    assert!(!path_is_under("adb://serial2", "adb://serial"), "a sibling sharing a prefix isn't inside");
    assert!(!path_is_under("mtp://dev/655370", "mtp://dev/65537"));
}
