//! The model every OS publishes: one `LocationInfo` per switcher row, the SMB facts a mount
//! source carries, and the network mount a path lies on. A leaf: it names nothing above it, so
//! the listing, the broadcast, and every arm can all depend on it.

use cmdr_fs::volume::published_locations::PublishedLocation;
use serde::{Deserialize, Serialize};

use cmdr_fs::volume::ConnectionState;

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
    MobileDevice,
}

/// Information about a location (volume, folder, or cloud drive).
///
/// Serialized Rust → frontend. It also derives `Deserialize` because it rides inside
/// the typed `volumes-changed` event payload (`VolumesChanged`), and `tauri_specta::Event`
/// requires the payload (and its nested types) to round-trip.
/// Fields serialized as explicit `null` when absent so specta's `validate_exported_command`
/// accepts the type in Unified mode.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    /// Base64-encoded WebP.
    pub icon: Option<String>,
    pub is_ejectable: bool,
    /// Filesystem type from `statfs` (for example, "apfs", "hfs", "smbfs").
    pub fs_type: Option<String>,
    /// Whether this volume supports macOS trash. Derived from `fs_type`.
    pub supports_trash: bool,
    /// Whether the MOUNT behind this location refuses writes right now: a read-only `.dmg`, a
    /// write-protected card, an MTP device reporting locked storage. From `MNT_RDONLY` via
    /// `read_only_from_statfs` for real mounts. Powers the 🔒 indicator and the copy/move write
    /// guard.
    ///
    /// Orthogonal to `capabilities.backend_can_write`, which answers whether the BACKEND serving
    /// this volume implements mutations at all. Both combinations happen: a writable backend on a
    /// read-only mount, and a read-only backend (`ArchiveVolume`) on a perfectly writable disk.
    pub mount_is_read_only: bool,
    /// Whether this volume is backed by a mounted disk image (a `.dmg`). Disk images are
    /// transient install-style mounts: the UI suppresses indexing (badge + first-connect
    /// prompt) and both free-space bars for them. Detected via DiskArbitration; see
    /// `disk_image::is_disk_image_mount`. Always `false` off macOS and for non-volume locations.
    pub is_disk_image: bool,
    /// Whether another switcher row sits on this volume's physical disk, so Eject leaves the disk
    /// attached and the row's menu also offers "Eject all". Set only by the listing
    /// (`volume_listing::complete`), since no one row can answer it.
    pub shares_disk_with_others: bool,
    /// Whether a cloud provider's own filesystem serves this mount (pCloud's `pcloudfs`,
    /// CloudMounter, …), so every entry read costs a round trip to that provider's daemon.
    /// Groups the row under CLOUD and, like `is_disk_image`, suppresses the index affordances:
    /// a drive index here would walk the provider's whole service to build something another
    /// device's sync invalidates. Set by `is_cloud_provider_mount` in BOTH `get_attached_volumes`
    /// and `resolve_path_volume_fast`, or the two drift.
    ///
    /// ❗ `false` for a `~/Library/CloudStorage` folder, which is an ordinary directory on the
    /// data volume that the cloud-drive arm publishes and the index reads at local speed.
    pub is_cloud_mount: bool,
    /// How live this volume's SESSION is: the switcher dot, the pane's connect
    /// views, and the reconnect subscription all read it. Set for every volume a
    /// connecting backend serves (SMB, SFTP, WebDAV, ADB) plus a saved-but-not-
    /// connected server; `None` for a local disk, a favorite, and the hub row.
    /// ❗ Not an "is this SMB" test — that is `Volume::backend_kind()`, backend-side.
    pub connection_state: Option<ConnectionState>,
    /// Whether the DEVICE behind this row is reachable, which is a different
    /// question from how live a session is. Set by the device providers only: a
    /// phone waiting for its "Allow USB debugging?" tap is present and must never
    /// start a reconnect backoff. `None` for everything that isn't a device.
    /// Whether this place belongs in the volume SWITCHER: the user's own cap on
    /// how many saved things crowd their disks. `Some` only on a server place
    /// (`server_volumes.rs`), which is the only row the cap applies to; `None`
    /// on a local disk, a favorite, and a mounted SMB share, all of which show
    /// unconditionally.
    ///
    /// ❗ The listing publishes EVERY saved place regardless, because a volume id
    /// with no row is one the app denies exists (a hub Enter and a restored tab
    /// both land on an id). Hiding is `navigation/volume-grouping.ts`'s job, and
    /// this field is what it reads.
    pub pinned: Option<bool>,
    /// Where opening this place lands, as an app path, when that isn't `path`: a
    /// server place's saved start folder (`server_volumes.rs`). `None` lands at
    /// `path`, and every row that isn't a server place carries `None`.
    ///
    /// ❗ Rides on the row so a pane picking a saved place goes straight to its
    /// landing without asking the store, and so the switcher and the pane read
    /// one spelling of it.
    pub landing_path: Option<String>,
    pub device_readiness: Option<cmdr_fs::volume::DeviceReadiness>,
    /// Negotiated USB link speed. Set only for MTP/mobile volumes; everything
    /// else carries `None`. Frontend maps to a label like "USB 3.2 Gen 1" and a
    /// theoretical max MB/s for the volume switcher.
    pub usb_speed: Option<crate::usb_speed::UsbSpeed>,
    /// What the backend registered for this volume can do (writable? can it be a
    /// copy source?), straight from `Volume::capabilities()`, so the frontend
    /// never re-derives capability from an id, an `fsType`, or a category.
    /// `None` when no backend is registered for this id (a favorite, or a volume
    /// discovery found before registration): the frontend falls back to its
    /// per-kind defaults. Filled by `enrich_from_volume_registry`, never by a
    /// discovery constructor.
    pub capabilities: Option<cmdr_fs::volume::VolumeCapabilities>,
    /// Single-letter menu shortcut, present only on favorite rows.
    pub favorite_shortcut: Option<String>,
    /// Present only on a favorite row: which volume it lives on and whether a pick can get there
    /// (`favorites/target.rs`). `None` on every other row.
    pub favorite_target: Option<crate::favorites::target::FavoriteTarget>,
    /// What a tab at this volume's root is called, when the mount directory's
    /// name isn't it: an SMB share's own name. `None` everywhere else, where the
    /// root folder's name is the label.
    ///
    /// ❗ A share whose name another server's mount already holds is mounted at a
    /// disambiguated path (`/Volumes/public-1`), and its tab read "public-1" while
    /// the header said "public on localhost:11482".
    pub root_label: Option<String>,
    /// The account a mounted SMB share is signed in as right now, as the mount
    /// table records it (`GUEST` for a guest mount). `None` for everything else.
    ///
    /// ❗ The LIVE account, which the hub shows while a share is connected: the saved
    /// row's account is for the next connect, and showing it read "Connected … as
    /// otheruser" over a mount signed in as testuser.
    pub mount_account: Option<String>,
}

/// Lets discovery collapse a doubly-mounted filesystem down to one published
/// location (`cmdr_fs::volume::canonical_root`).
impl cmdr_fs::volume::canonical_root::MountRootCandidate for LocationInfo {
    fn volume_id(&self) -> &str {
        &self.id
    }

    fn mount_root(&self) -> &str {
        &self.path
    }
}

impl PublishedLocation for LocationInfo {
    fn location_id(&self) -> &str {
        &self.id
    }

    fn location_path(&self) -> &str {
        &self.path
    }

    fn is_favorite(&self) -> bool {
        self.category == LocationCategory::Favorite
    }
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// The volume-shaped name for the same struct: discovery calls every entry a
/// location, and everything downstream of it calls the ones backed by a mount a
/// volume.
pub use LocationInfo as VolumeInfo;

/// Information about an SMB mount, from its mount source (`statfs` on macOS, `/proc/mounts` on
/// Linux) or a GVFS share's folder name.
#[derive(Debug, Clone)]
pub struct SmbMountInfo {
    /// Server hostname or IP (for example, "192.168.1.111").
    pub server: String,
    /// Share name, ONE path segment (for example, "naspi"). This is what goes to
    /// TreeConnect, so it can never carry a separator.
    pub share: String,
    /// Where inside the share this mount is anchored, `/`-separated and without
    /// leading or trailing separators (for example, "example.com"). `None` for the
    /// ordinary mount, which sits at the share root.
    ///
    /// macOS follows a DFS referral by making a SECOND mount underneath the
    /// namespace root (`/Volumes/SYSVOL/example.com` beside `/Volumes/SYSVOL`),
    /// and a subdirectory mount looks the same; on Linux, `mount -t cifs
    /// //server/share/sub` records the whole path in the device field. Both are
    /// the same SHARE, so both derive the same volume ID and ride one session; the
    /// anchor is what a path on this mount has to be joined onto before it goes on
    /// the wire, and it reaches the backend as `SmbVolume`'s `share_root`.
    pub subpath: Option<String>,
    /// Username if present in the mount source (for example, "david").
    pub username: Option<String>,
    /// Port from the mount source (for example, 10480). Defaults to 445.
    pub port: u16,
}

/// The network mount a path lexically lies on, read from the kernel's cached mount table
/// (`getfsstat(MNT_NOWAIT)` on macOS, `/proc/mounts` on Linux): ❌ never a `statfs` on the path,
/// which on a slow share waits on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkMount {
    /// Where it's mounted, for example `/Volumes/naspi`.
    pub mount_point: String,
    /// What the switcher calls it, from the table alone (an SMB share reads "share on server").
    pub name: String,
}
