//! What the volume list asks a device backend, and how a device backend says
//! "the device set changed".
//!
//! The generalization of the MTP-only fold `volume_listing::complete` used to
//! hardcode. A device backend (MTP, ADB) registers one [`DeviceVolumeProvider`]
//! at startup; the listing folds over every provider, eject asks which provider
//! owns a volume id, and path resolution asks which provider's entry a
//! `scheme://` path falls under. On hotplug a provider calls
//! [`notify_devices_changed`], which is the one push channel the frontend has.
//!
//! ❗ [`DeviceVolumeProvider::entries`] answers from CACHED state, never the
//! wire: the listing runs on every `volumes-changed`, and a provider that probes
//! its devices there turns a refresh into a round of USB or socket traffic.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::usb_speed::UsbSpeed;
use crate::volume_listing::{LocationCategory, LocationInfo};

/// One device storage as its provider lists it. [`append_from`] turns it into
/// the `LocationInfo` the frontend reads, filling in everything a mobile device
/// has in common.
#[derive(Debug, Clone)]
pub(crate) struct DeviceVolumeEntry {
    /// The volume id the registry files the backend under.
    pub id: String,
    /// What the picker shows.
    pub name: String,
    /// The `scheme://…` root a pane navigates to.
    pub path: String,
    /// The `fs_type` the frontend branches on (`"mtp"`, `"adb"`).
    pub fs_type: &'static str,
    /// Whether the storage refuses writes.
    pub mount_is_read_only: bool,
    /// The USB link speed, when the backend can see it.
    pub usb_speed: Option<UsbSpeed>,
}

/// A backend that turns attached devices into volumes.
///
/// Every method answers from the provider's own cached state. Boxed futures
/// rather than `async fn` so the trait stays object-safe: the registry holds
/// `Arc<dyn DeviceVolumeProvider>`.
pub(crate) trait DeviceVolumeProvider: Send + Sync + 'static {
    /// The provider's stable name (`"mtp"`, `"adb"`), the key the registry
    /// dedupes on and what eject reports back.
    fn id(&self) -> &'static str;

    /// Every storage the provider currently offers. ❗ From cached state, never
    /// the wire.
    fn entries(&self) -> Pin<Box<dyn Future<Output = Vec<DeviceVolumeEntry>> + Send + '_>>;

    /// Whether `volume_id` names a device this provider has live. ❌ Never an
    /// id-shape guess: an id that LOOKS like one of ours but names a device that
    /// left would otherwise route an eject at nothing.
    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

    /// `(total, available)` bytes for the storage `path` falls under, or `None`
    /// when the path isn't one of this provider's.
    fn space_for_path<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<(u64, u64)>> + Send + 'a>>;

    /// Retires the volume `volume_id` names. The `Err` string is the backend's
    /// own diagnostic; it reaches the log and the details line, ❌ never the
    /// message.
    fn eject<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

/// The registered providers, in registration order.
static PROVIDERS: LazyLock<RwLock<Vec<Arc<dyn DeviceVolumeProvider>>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Files a provider under its `id()`. The first registration per id wins; a
/// duplicate is logged and dropped, so a double `install_device_provider` can't
/// list every device twice.
pub(crate) fn register_device_provider(provider: Arc<dyn DeviceVolumeProvider>) {
    let mut providers = PROVIDERS.write_ignore_poison();
    if providers.iter().any(|p| p.id() == provider.id()) {
        log::warn!(target: "volume", "device provider {} registered twice; keeping the first", provider.id());
        return;
    }
    log::debug!(target: "volume", "registered device provider {}", provider.id());
    providers.push(provider);
}

/// A snapshot of the registered providers.
pub(crate) fn device_providers() -> Vec<Arc<dyn DeviceVolumeProvider>> {
    PROVIDERS.read_ignore_poison().clone()
}

/// Appends every registered provider's storages to `volumes`.
pub(crate) async fn append_device_volumes(volumes: &mut Vec<LocationInfo>) {
    append_from(volumes, &device_providers()).await;
}

/// Appends `providers`' storages to `volumes`, each as a `MobileDevice` entry.
///
/// What every device storage has in common lives here, once: ejectable, no
/// trash, no icon, not a disk image, no SMB state, and `capabilities: None`
/// because enrichment fills that from the registered `Volume` afterwards.
pub(crate) async fn append_from(volumes: &mut Vec<LocationInfo>, providers: &[Arc<dyn DeviceVolumeProvider>]) {
    for provider in providers {
        for entry in provider.entries().await {
            volumes.push(location_from_entry(entry));
        }
    }
}

fn location_from_entry(entry: DeviceVolumeEntry) -> LocationInfo {
    LocationInfo {
        id: entry.id,
        name: entry.name,
        path: entry.path,
        category: LocationCategory::MobileDevice,
        icon: None,
        is_ejectable: true,
        mount_is_read_only: entry.mount_is_read_only,
        is_disk_image: false,
        fs_type: Some(entry.fs_type.to_string()),
        supports_trash: false,
        smb_connection_state: None,
        usb_speed: entry.usb_speed,
        capabilities: None,
    }
}

/// What a provider calls when its device set changed: logs it and republishes
/// the volume list.
pub(crate) fn notify_devices_changed(provider_id: &str) {
    log::debug!(target: "volume", "device provider {provider_id} reports a device change");
    crate::volume_broadcast::emit_volumes_changed();
}

/// The provider that has `volume_id` live, if any.
pub(crate) async fn provider_for_volume_id(volume_id: &str) -> Option<Arc<dyn DeviceVolumeProvider>> {
    for provider in device_providers() {
        if provider.owns_volume_id(volume_id).await {
            return Some(provider);
        }
    }
    None
}

/// Live `(total, available)` bytes for a device path, from whichever provider
/// claims it.
pub(crate) async fn device_space_for_path(path: &str) -> Option<(u64, u64)> {
    for provider in device_providers() {
        if let Some(space) = provider.space_for_path(path).await {
            return Some(space);
        }
    }
    None
}

/// The device volume `path` falls under: the entry whose `path` equals it or
/// is a `/`-separated prefix of it.
pub(crate) async fn device_volume_for_path(path: &str) -> Option<LocationInfo> {
    let mut volumes = Vec::new();
    append_device_volumes(&mut volumes).await;
    volumes.into_iter().find(|v| path_is_under(path, &v.path))
}

/// Whether `path` is `root` itself or something inside it.
fn path_is_under(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests;
