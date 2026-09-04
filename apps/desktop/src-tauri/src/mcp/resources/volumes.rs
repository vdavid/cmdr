//! The `cmdr://state` volumes section builder.
//!
//! Every volume renders through one uniform shape so agents stop guessing which
//! entries carry ids or what a bare string meant: `name`, `id`, and `kind`
//! (`local` / `smb` / `mtp` / `virtual`) always, plus the present-when-known
//! `filesystem`, `readOnly`, `ejectable`, `indexStatus`, `smbConnectionState`,
//! `totalBytes` / `availableBytes`, and their spelled-out twins `totalHuman` /
//! `availableHuman`.
//!
//! Same snapshot-then-format split as `resources/indexing.rs`: [`build_volumes_yaml`]
//! is pure over a `&[VolumeSummary]` so the formatting is unit-testable without a
//! live app, while [`snapshot_volumes`] does the live reads (the volume layer's
//! `list_locations` + SMB-state enrichment, the MTP connection manager, and the
//! per-volume index freshness). `indexStatus` shares the one `status_token`
//! mapping with `cmdr://indexing`, so a volume can't read `fresh` in one resource
//! and `stale` in the other.

use super::indexing::status_token;
use crate::file_system::volume::SpaceInfo;
use crate::index_host::index;
use crate::search::format_size;

/// A volume's transport kind, the coarse routing hint an agent reads first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VolumeKind {
    /// A local disk, favorite folder, or cloud-drive mount (a real filesystem path).
    Local,
    /// An SMB share (direct smb2 or an OS mount). Constructed only in the macOS
    /// `snapshot_volumes` path (the Linux snapshot surfaces only root + MTP), so
    /// off macOS it's genuinely unconstructed — the `token` match still needs it.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(
            dead_code,
            reason = "constructed only in the macOS snapshot_volumes path; the token match still needs the variant off macOS"
        )
    )]
    Smb,
    /// An MTP device storage (Android / camera over USB).
    Mtp,
    /// A synthetic entry with no backing device (the `Network` browser root). Also
    /// macOS-path-only today, so off macOS it's unconstructed — see `Smb`.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "macOS-path-only today; unconstructed off macOS, see `Smb`")
    )]
    Virtual,
}

impl VolumeKind {
    pub(crate) fn token(self) -> &'static str {
        match self {
            VolumeKind::Local => "local",
            VolumeKind::Smb => "smb",
            VolumeKind::Mtp => "mtp",
            VolumeKind::Virtual => "virtual",
        }
    }
}

/// One volume, snapshotted into plain data so the text builder stays pure.
#[derive(Debug, Clone)]
pub(crate) struct VolumeSummary {
    pub name: String,
    pub id: String,
    pub kind: VolumeKind,
    /// Filesystem type (`apfs`, `exfat`, `smbfs`, …). `None` for kinds that have
    /// no filesystem (MTP, virtual).
    pub filesystem: Option<String>,
    /// Whether the volume is mounted read-only. `None` when not applicable.
    pub read_only: Option<bool>,
    /// Whether the volume can be ejected (routes the `eject` tool). `None` when
    /// not applicable.
    pub ejectable: Option<bool>,
    /// Index freshness token (`fresh` / `scanning` / `stale` / `off`), shared with
    /// `cmdr://indexing`. `None` for kinds that are never indexed (virtual).
    pub index_status: Option<&'static str>,
    /// SMB connection state (`direct` / `os_mount` / `disconnected`); `None` off SMB.
    pub smb_connection_state: Option<&'static str>,
    /// Where the volume is mounted, the path a search scope names to cover this
    /// drive. `None` for a volume with no filesystem path (MTP storages, the
    /// synthetic `Network` root), which is also exactly where a search can't reach.
    ///
    /// ❌ Not rendered into the `cmdr://state` YAML: that view redacts home paths,
    /// and a favorite folder's mount path would come out redacted, so an AI client
    /// would copy a scope that matches nothing. The agent's `list_volumes` reads it
    /// unredacted instead.
    pub mount_path: Option<String>,
    /// What the volume reports about its room, from the space poller's cache.
    /// `None` when nothing is watching this volume — see [`space_summary`] for
    /// why that isn't a `statfs` here.
    pub space: Option<SpaceInfo>,
}

/// The volume's space from the poller's cache, never a fresh `statfs`.
///
/// "How full is this drive" must not be able to block a resource read or a tool
/// call: `statfs` on a hung network mount waits 30–120 s, and `cmdr://state` is
/// read constantly. The poller already holds a live value for every volume
/// something is watching (the boot volume always, plus whatever the panes show),
/// which covers the volume a disk-space question is actually about. Anything
/// unwatched reports no space at all rather than a stale or guessed number.
pub(crate) fn space_summary(volume_id: &str) -> Option<SpaceInfo> {
    crate::space_poller::cached_space(volume_id)
}

/// Push one volume's YAML block. `name`, `id`, and `kind` always render; the rest
/// only when known, so a bare local disk stays terse and an SMB share carries its
/// connection state.
fn push_volume(lines: &mut Vec<String>, v: &VolumeSummary) {
    lines.push(format!("  - name: {}", v.name));
    lines.push(format!("    id: {}", v.id));
    lines.push(format!("    kind: {}", v.kind.token()));
    if let Some(ref fs) = v.filesystem {
        lines.push(format!("    filesystem: {}", fs));
    }
    if let Some(read_only) = v.read_only {
        lines.push(format!("    readOnly: {}", read_only));
    }
    if let Some(ejectable) = v.ejectable {
        lines.push(format!("    ejectable: {}", ejectable));
    }
    if let Some(status) = v.index_status {
        lines.push(format!("    indexStatus: {}", status));
    }
    if let Some(state) = v.smb_connection_state {
        lines.push(format!("    smbConnectionState: {}", state));
    }
    // Raw bytes AND a formatted size, never one instead of the other. The raw pair
    // is what a reader does arithmetic with ("is 40 GB of downloads worth
    // deleting"), and a pre-rounded "1.4 TB" would lose that; the human pair is
    // what a reader who can't run a script states out loud, and dividing by 1,024
    // in its head is where it invents numbers. Formatted through
    // `search::format_size`, the one formatter (so a size reads the same here as in
    // the `search` table).
    if let Some(space) = v.space {
        if let SpaceInfo::Bounded {
            total_bytes,
            available_bytes,
            ..
        } = space
        {
            lines.push(format!("    totalBytes: {total_bytes}"));
            lines.push(format!("    totalHuman: {}", format_size(total_bytes)));
            lines.push(format!("    availableBytes: {available_bytes}"));
            lines.push(format!("    availableHuman: {}", format_size(available_bytes)));
        } else {
            // No ceiling (a quota-less WebDAV account). Saying so beats omitting
            // the block: a reader who sees only `usedBytes` would otherwise wonder
            // whether the capacity was simply unknown.
            lines.push("    unbounded: true".to_string());
        }
        lines.push(format!("    usedBytes: {}", space.used_bytes()));
        lines.push(format!("    usedHuman: {}", format_size(space.used_bytes())));
    }
}

/// Build the `volumes:` YAML section, one uniform block per volume. Pure over the
/// snapshot; `now`-free (nothing time-relative here).
pub(crate) fn build_volumes_yaml(volumes: &[VolumeSummary]) -> String {
    if volumes.is_empty() {
        return "volumes: []\n".to_string();
    }
    let mut lines = vec!["volumes:".to_string()];
    for v in volumes {
        push_volume(&mut lines, v);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Map a volume's live index status to the section's `indexStatus` token, reusing
/// the one `cmdr://indexing` mapping.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn index_status_token(status: &cmdr_index::VolumeIndexStatus) -> &'static str {
    status_token(status.enabled, status.freshness)
}

/// Snapshot every volume for the `cmdr://state` `volumes:` section: local / SMB
/// locations (with SMB connection state and per-volume index freshness) and MTP
/// device storages, plus the synthetic `Network` browser root. The impure half;
/// [`build_volumes_yaml`] formats the result.
pub(crate) async fn snapshot_volumes() -> Vec<VolumeSummary> {
    let mut out: Vec<VolumeSummary> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Off-thread + timeout-guarded: `list_locations` runs blocking macOS
        // metadata syscalls, and a resource read must never wedge the MCP handler
        // (a dying mount once made `cmdr://state` reads take a flat 30s). SMB-state
        // enrichment runs inside the same guarded closure so the whole snapshot is
        // one bounded unit. Mirrors `volume_broadcast::do_emit`'s guard. See
        // `volumes/DETAILS.md` § "Hung mounts".
        let snapshot = tokio::task::spawn_blocking(|| {
            let mut locations = crate::volumes::list_locations();
            // Enrich from the `VolumeManager` so agents see the SMB connection
            // state (`direct` / `os_mount` / `disconnected`) alongside the rest.
            crate::volumes::enrich_from_volume_registry(&mut locations);
            locations
        });
        let locations = match tokio::time::timeout(std::time::Duration::from_secs(2), snapshot).await {
            Ok(Ok(locations)) => locations,
            _ => Vec::new(),
        };
        for loc in &locations {
            let smb_connection_state = loc.smb_connection_state.map(|state| match state {
                crate::volumes::SmbConnectionState::Direct => "direct",
                crate::volumes::SmbConnectionState::OsMount => "os_mount",
                crate::volumes::SmbConnectionState::Disconnected => "disconnected",
            });
            let is_smb = smb_connection_state.is_some() || crate::volumes::is_smb_fs_type(loc.fs_type.as_deref());
            let kind = if is_smb { VolumeKind::Smb } else { VolumeKind::Local };
            // Path-based status resolution routes each volume to its OWN index (see
            // `indexing::routing::volume_id_for_local_path`): a mounted-but-unindexed
            // external drive (`/Volumes/X`) reports `off`, not `root`'s freshness, so
            // this can't disagree with `cmdr://indexing`.
            let status = index().volume_status_for_path(&loc.path);
            out.push(VolumeSummary {
                name: loc.name.clone(),
                id: loc.id.clone(),
                kind,
                filesystem: loc.fs_type.clone(),
                read_only: Some(loc.mount_is_read_only),
                ejectable: Some(loc.is_ejectable),
                index_status: Some(index_status_token(&status)),
                smb_connection_state,
                mount_path: Some(loc.path.clone()),
                space: space_summary(&loc.id),
            });
        }
        // The `Network` browser root is a synthetic navigation target, not a
        // device: no filesystem, ejectability, or index.
        out.push(VolumeSummary {
            name: "Network".to_string(),
            id: "network".to_string(),
            kind: VolumeKind::Virtual,
            filesystem: None,
            read_only: None,
            ejectable: None,
            index_status: None,
            smb_connection_state: None,
            mount_path: None,
            space: None,
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let status = index().volume_status(cmdr_index::ROOT_VOLUME_ID);
        out.push(VolumeSummary {
            name: "root".to_string(),
            id: cmdr_index::ROOT_VOLUME_ID.to_string(),
            kind: VolumeKind::Local,
            filesystem: None,
            read_only: None,
            ejectable: None,
            index_status: Some(status_token(status.enabled, status.freshness)),
            smb_connection_state: None,
            mount_path: Some("/".to_string()),
            space: space_summary(cmdr_index::ROOT_VOLUME_ID),
        });
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
        for device_info in &devices {
            let has_multiple = device_info.storages.len() > 1;
            let device_name = device_info
                .device
                .product
                .as_deref()
                .or(device_info.device.manufacturer.as_deref())
                .unwrap_or(&device_info.device.id);
            for storage in &device_info.storages {
                let name = if has_multiple {
                    format!("{} - {}", device_name, storage.name)
                } else {
                    device_name.to_string()
                };
                // MTP volume id is `{device_id}:{storage_id}`, the same id the
                // index and the `eject` tool take (identity.rs::mtp_volume_id).
                let volume_id = format!("{}:{}", device_info.device.id, storage.id);
                let status = index().volume_status(&volume_id);
                out.push(VolumeSummary {
                    name,
                    id: volume_id.clone(),
                    kind: VolumeKind::Mtp,
                    filesystem: None,
                    read_only: Some(storage.is_read_only),
                    ejectable: Some(true),
                    index_status: Some(index_status_token(&status)),
                    smb_connection_state: None,
                    // An MTP storage has no filesystem path to scope a search with.
                    mount_path: None,
                    space: space_summary(&volume_id),
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(name: &str, id: &str) -> VolumeSummary {
        VolumeSummary {
            name: name.to_string(),
            id: id.to_string(),
            kind: VolumeKind::Local,
            filesystem: Some("apfs".to_string()),
            read_only: Some(false),
            ejectable: Some(false),
            index_status: Some("fresh"),
            smb_connection_state: None,
            mount_path: Some("/".to_string()),
            space: None,
        }
    }

    #[test]
    fn empty_renders_flow_seq() {
        assert_eq!(build_volumes_yaml(&[]), "volumes: []\n");
    }

    #[test]
    fn local_volume_carries_every_known_field() {
        let yaml = build_volumes_yaml(&[local("Macintosh HD", "root")]);
        assert_eq!(
            yaml,
            "volumes:\n  - name: Macintosh HD\n    id: root\n    kind: local\n    \
             filesystem: apfs\n    readOnly: false\n    ejectable: false\n    indexStatus: fresh\n"
        );
    }

    #[test]
    fn the_mount_path_stays_out_of_the_yaml() {
        // This view redacts home paths, so a favorite folder's mount path would render
        // redacted and an AI client copying it into a search scope would match nothing.
        // The agent reads the unredacted path from `list_volumes` instead.
        let mut v = local("Downloads", "favorite-downloads");
        v.mount_path = Some("/Users/someone/Downloads".to_string());
        let yaml = build_volumes_yaml(&[v]);
        assert!(!yaml.contains("mountPath"));
        assert!(!yaml.contains("/Users/someone/Downloads"));
    }

    #[test]
    fn a_watched_volume_carries_capacity_and_free_space() {
        // The pair is what makes a size answer actionable: "40 GB" reads
        // differently against 2 TB free than against 8 GB.
        let mut v = local("Macintosh HD", "root");
        v.space = Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000));
        let yaml = build_volumes_yaml(&[v]);
        assert!(yaml.contains("totalBytes: 2000000000000"));
        assert!(yaml.contains("availableBytes: 214300000000"));
        // Beside the bytes, not instead of them: a reader that has to state the number
        // out loud can't divide by 1,024 reliably, and one doing arithmetic still has
        // the exact value.
        assert!(yaml.contains(&format!("totalHuman: {}", format_size(2_000_000_000_000))));
        assert!(yaml.contains(&format!("availableHuman: {}", format_size(214_300_000_000))));
    }

    #[test]
    fn an_unwatched_volume_omits_space_rather_than_reporting_zero() {
        // Nothing is polling it, so we don't know. A rendered `availableBytes: 0`
        // would read as a full disk.
        let yaml = build_volumes_yaml(&[local("Backup HD", "volumes-backup")]);
        assert!(!yaml.contains("totalBytes"));
        assert!(!yaml.contains("availableBytes"));
        assert!(!yaml.contains("totalHuman"));
        assert!(!yaml.contains("availableHuman"));
    }

    #[test]
    fn smb_volume_carries_connection_state_and_kind() {
        let smb = VolumeSummary {
            name: "naspi".to_string(),
            id: "smb-192-168-1-111-445-naspi".to_string(),
            kind: VolumeKind::Smb,
            filesystem: Some("smbfs".to_string()),
            read_only: Some(false),
            ejectable: Some(true),
            index_status: Some("stale"),
            smb_connection_state: Some("direct"),
            mount_path: Some("/Volumes/naspi".to_string()),
            space: None,
        };
        let yaml = build_volumes_yaml(&[smb]);
        assert!(yaml.contains("kind: smb"));
        assert!(yaml.contains("smbConnectionState: direct"));
        assert!(yaml.contains("indexStatus: stale"));
        assert!(yaml.contains("ejectable: true"));
    }

    #[test]
    fn mtp_volume_omits_filesystem_and_smb_state() {
        let mtp = VolumeSummary {
            name: "Pixel 8".to_string(),
            id: "mtp-336592896:65537".to_string(),
            kind: VolumeKind::Mtp,
            filesystem: None,
            read_only: Some(true),
            ejectable: Some(true),
            index_status: Some("off"),
            smb_connection_state: None,
            mount_path: None,
            space: None,
        };
        let yaml = build_volumes_yaml(&[mtp]);
        assert!(yaml.contains("kind: mtp"));
        assert!(yaml.contains("id: mtp-336592896:65537"));
        assert!(yaml.contains("readOnly: true"));
        assert!(!yaml.contains("filesystem:"));
        assert!(!yaml.contains("smbConnectionState:"));
    }

    #[test]
    fn virtual_volume_is_name_id_kind_only() {
        let network = VolumeSummary {
            name: "Network".to_string(),
            id: "network".to_string(),
            kind: VolumeKind::Virtual,
            filesystem: None,
            read_only: None,
            ejectable: None,
            index_status: None,
            smb_connection_state: None,
            mount_path: None,
            space: None,
        };
        let yaml = build_volumes_yaml(&[network]);
        assert_eq!(
            yaml,
            "volumes:\n  - name: Network\n    id: network\n    kind: virtual\n"
        );
    }

    #[test]
    fn mixed_fixture_keeps_every_entry_uniform_head() {
        let network = VolumeSummary {
            name: "Network".to_string(),
            id: "network".to_string(),
            kind: VolumeKind::Virtual,
            filesystem: None,
            read_only: None,
            ejectable: None,
            index_status: None,
            smb_connection_state: None,
            mount_path: None,
            space: None,
        };
        let yaml = build_volumes_yaml(&[local("Macintosh HD", "root"), network]);
        // Every entry leads with name / id / kind, in order.
        let heads: Vec<&str> = yaml
            .lines()
            .filter(|l| l.trim_start().starts_with("- name:") || l.contains("id:") || l.contains("kind:"))
            .collect();
        assert_eq!(
            heads,
            vec![
                "  - name: Macintosh HD",
                "    id: root",
                "    kind: local",
                "  - name: Network",
                "    id: network",
                "    kind: virtual",
            ]
        );
    }
}
