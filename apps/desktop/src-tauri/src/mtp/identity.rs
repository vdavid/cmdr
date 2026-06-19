//! MTP device and volume identity: stable ids and robust parsing.
//!
//! ## Why a stable id matters (plan rabbit hole #1)
//!
//! An MTP device id keys the live session registry AND the persisted per-volume
//! index DB (`index-{volume_id}.db`). For the index to survive a replug, the id
//! must be STABLE across reconnects. The USB topology `location_id` is stable
//! only for a given port: replug into a different port and it changes, so a
//! topology-keyed index forces a full rescan on every reconnection — gutting the
//! feature. Many Android devices in MTP mode DO report a stable `serial_number`,
//! so [`device_id_for`] prefers it, falling back to `location_id` (with a
//! documented "same-port-only" limitation) when absent.
//!
//! ## Why parsing must be `:`-robust (the riskiest part)
//!
//! The volume id is `{device_id}:{storage_id}` and is split on `:` at several
//! call sites to recover the device id and storage id. A device id built from a
//! serial CAN contain a `:` (some devices report serials with colons), which a
//! naive `split(':').nth(1)` mis-parses (it takes the SECOND segment, not the
//! storage). The storage id is ALWAYS the trailing numeric component, so the one
//! robust split is from the RIGHT ([`rsplit_once(':')`](str::rsplit_once)):
//! everything before the last `:` is the device id, the tail is the storage id.
//! [`split_volume_id`] is the single funnel every parser must use, so a `:` in a
//! serial never breaks classification (`.claude/rules/no-string-matching.md`:
//! structured parse over substring branching). The serial is otherwise OPAQUE —
//! we never interpret its contents, only round-trip it.

/// The `mtp-` prefix every MTP device id carries, so a volume id is recognizable
/// as MTP and distinct from `root` / SMB ids.
pub(crate) const MTP_DEVICE_ID_PREFIX: &str = "mtp-";

/// Build the stable MTP device id for a device, preferring its serial number.
///
/// - With a non-empty serial: `mtp-{serial}` (stable across replug to ANY port).
/// - Without (or an empty serial): `mtp-{location_id}` (stable for the SAME port
///   only — a different-port replug changes it and forces a rescan).
///
/// The serial is taken verbatim (it may contain a `:`; parsing stays robust via
/// [`split_volume_id`]). An all-whitespace serial is treated as absent.
pub(crate) fn device_id_for(serial: Option<&str>, location_id: u64) -> String {
    match serial.map(str::trim).filter(|s| !s.is_empty()) {
        Some(serial) => format!("{MTP_DEVICE_ID_PREFIX}{serial}"),
        None => format!("{MTP_DEVICE_ID_PREFIX}{location_id}"),
    }
}

/// Build the MTP volume id from a device id and storage id:
/// `{device_id}:{storage_id}`. The storage id is numeric and trails, so
/// [`split_volume_id`] recovers both halves even when the device id holds a `:`.
pub(crate) fn mtp_volume_id(device_id: &str, storage_id: u32) -> String {
    format!("{device_id}:{storage_id}")
}

/// Split a `{device_id}:{storage_id}` MTP volume id into its parts, robustly.
///
/// Splits on the LAST `:` so a device id built from a serial containing `:`
/// round-trips correctly (the storage id is always the trailing numeric tail).
/// Returns `None` if there's no `:` or the tail isn't a `u32`.
///
/// This is the ONE place volume-id parsing happens; every caller that needs the
/// device id or storage id goes through here rather than re-implementing a split.
pub(crate) fn split_volume_id(volume_id: &str) -> Option<(&str, u32)> {
    let (device_id, storage_str) = volume_id.rsplit_once(':')?;
    let storage_id = storage_str.parse::<u32>().ok()?;
    Some((device_id, storage_id))
}

/// The device id half of an MTP volume id (`{device_id}:{storage_id}`), or
/// `None` if the id isn't a well-formed MTP volume id. Convenience over
/// [`split_volume_id`] for callers that only need the device.
pub(crate) fn device_id_of_volume(volume_id: &str) -> Option<&str> {
    split_volume_id(volume_id).map(|(device_id, _)| device_id)
}

/// The storage id half of an MTP volume id, or `None` if malformed. Convenience
/// over [`split_volume_id`] for callers that only need the storage.
pub(crate) fn storage_id_of_volume(volume_id: &str) -> Option<u32> {
    split_volume_id(volume_id).map(|(_, storage_id)| storage_id)
}

/// Whether `id` looks like an MTP device id (carries the `mtp-` prefix). A cheap
/// shape check; it does NOT prove the device is connected.
pub(crate) fn is_mtp_device_id(id: &str) -> bool {
    id.starts_with(MTP_DEVICE_ID_PREFIX)
}

/// Whether `volume_id` is a well-formed MTP volume id: an `mtp-`-prefixed device
/// id plus a numeric storage tail. Shape-only (doesn't prove the volume exists).
pub(crate) fn is_mtp_volume_id(volume_id: &str) -> bool {
    split_volume_id(volume_id).is_some_and(|(device_id, _)| is_mtp_device_id(device_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── device_id_for: serial preferred, location fallback ────────────────

    #[test]
    fn prefers_serial_when_present() {
        assert_eq!(device_id_for(Some("ABC123"), 336_592_896), "mtp-ABC123");
    }

    #[test]
    fn falls_back_to_location_id_without_serial() {
        assert_eq!(device_id_for(None, 336_592_896), "mtp-336592896");
    }

    #[test]
    fn treats_empty_or_whitespace_serial_as_absent() {
        // A device that reports an empty/blank serial must fall back to the
        // topology id rather than producing the degenerate `mtp-` id.
        assert_eq!(device_id_for(Some(""), 42), "mtp-42");
        assert_eq!(device_id_for(Some("   "), 42), "mtp-42");
    }

    #[test]
    fn serial_with_colon_is_taken_verbatim() {
        // The whole point of the robust parse: a serial may contain a `:`. The id
        // keeps it; round-tripping through split_volume_id still works (below).
        let id = device_id_for(Some("AA:BB:CC"), 7);
        assert_eq!(id, "mtp-AA:BB:CC");
    }

    // ── split_volume_id: the `:`-in-serial robustness (the riskiest part) ──

    #[test]
    fn splits_a_plain_location_volume_id() {
        assert_eq!(split_volume_id("mtp-336592896:65537"), Some(("mtp-336592896", 65537)));
    }

    #[test]
    fn splits_a_serial_volume_id_without_colon() {
        assert_eq!(split_volume_id("mtp-ABC123:65537"), Some(("mtp-ABC123", 65537)));
    }

    #[test]
    fn splits_a_serial_volume_id_that_contains_colons() {
        // The headline case. With `mtp-AA:BB:CC` as the device id and 65537 as the
        // storage, a naive `split(':').nth(1)` would return "BB" and fail the u32
        // parse. rsplit_once takes the LAST `:`, recovering device + storage right.
        let device_id = device_id_for(Some("AA:BB:CC"), 0);
        let volume_id = mtp_volume_id(&device_id, 65537);
        assert_eq!(volume_id, "mtp-AA:BB:CC:65537");
        assert_eq!(split_volume_id(&volume_id), Some(("mtp-AA:BB:CC", 65537)));
    }

    #[test]
    fn rejects_a_volume_id_without_a_colon() {
        assert_eq!(split_volume_id("mtp-noStorage"), None);
    }

    #[test]
    fn rejects_a_non_numeric_storage_tail() {
        // The tail after the last `:` must be a u32. A device id whose serial ends
        // in `:something-nonnumeric` and that has NO real storage tail is rejected
        // rather than mis-read.
        assert_eq!(split_volume_id("mtp-AA:BB"), None);
    }

    #[test]
    fn device_and_storage_convenience_accessors() {
        let volume_id = "mtp-AA:BB:CC:65537";
        assert_eq!(device_id_of_volume(volume_id), Some("mtp-AA:BB:CC"));
        assert_eq!(storage_id_of_volume(volume_id), Some(65537));
        assert_eq!(device_id_of_volume("not-mtp"), None);
        assert_eq!(storage_id_of_volume("not-mtp"), None);
    }

    // ── id classification ─────────────────────────────────────────────────

    #[test]
    fn recognizes_mtp_device_and_volume_ids() {
        assert!(is_mtp_device_id("mtp-ABC"));
        assert!(!is_mtp_device_id("root"));
        assert!(!is_mtp_device_id("smb-nas:445:share"));

        assert!(is_mtp_volume_id("mtp-ABC:65537"));
        assert!(is_mtp_volume_id("mtp-AA:BB:CC:65537"));
        // An SMB volume id is `{server}:{port}:{share}` — the tail isn't numeric
        // ... unless a share is all-digits, but it never carries the mtp- prefix,
        // so the device-id prefix check excludes it.
        assert!(!is_mtp_volume_id("smb-host:445:1234"));
        assert!(!is_mtp_volume_id("root"));
    }

    #[test]
    fn port_change_changes_a_location_id_but_not_a_serial_id() {
        // Same device, two ports: a serial-based id is identical (index re-matches
        // on replug), a location-based id differs (forces a rescan). This is the
        // exact behavior the identity fix buys.
        assert_eq!(
            device_id_for(Some("SERIAL1"), 100),
            device_id_for(Some("SERIAL1"), 200),
            "a serial id is port-independent",
        );
        assert_ne!(
            device_id_for(None, 100),
            device_id_for(None, 200),
            "a location id changes with the port",
        );
    }
}
