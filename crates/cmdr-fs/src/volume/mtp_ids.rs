//! MTP device and volume identity: stable ids and robust parsing.
//!
//! Sits beside [`smb_volume_id`](super::smb_volume_id) because it's the same kind of thing: the
//! vocabulary for naming a volume, needed by the index and by the app's MTP session
//! layer alike. Pure string work over `std` — no device, no session, no I/O.
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
//! ## The `{device_id}:{storage_id}` shape
//!
//! A volume id is `{device_id}:{storage_id}`, split at several call sites to
//! recover each half. [`split_volume_id`] is the single funnel every parser must
//! use (`.claude/rules/no-string-matching.md`: structured parse over substring
//! branching), and it splits from the RIGHT, because the storage id is always the
//! trailing numeric component.
//!
//! Two layers keep that honest. The device id comes out of the id funnel
//! ([`super::ids::mtp_device_id`]), so it holds only alphanumerics and dashes no
//! matter what the device reports: a serial with a `:` in it (some devices do)
//! can't shift the split, and a serial with a `/` or a `.` can't break the
//! `index-{volume_id}.db` filename. The right-split then holds even if that ever
//! regressed. The serial itself is OPAQUE — we never interpret its contents.

/// The `mtp-` prefix every MTP device id carries, so a volume id is recognizable
/// as MTP and distinct from `root` / SMB ids. Must match the scheme
/// [`super::ids::mtp_device_id`] mints under; `prefix_matches_the_id_funnel`
/// holds the two together.
pub const MTP_DEVICE_ID_PREFIX: &str = "mtp-";

/// Build the stable MTP device id for a device, preferring its serial number.
///
/// - With a non-empty serial: keyed by the serial (stable across replug to ANY port).
/// - Without (or an empty serial): keyed by the `location_id` (stable for the
///   SAME port only — a different-port replug changes it and forces a rescan).
///
/// An all-whitespace serial is treated as absent. The serial itself is OPAQUE:
/// it goes through [`super::ids::mtp_device_id`] rather than into the id
/// verbatim, so a serial carrying a `/`, a `.`, or a `:` can't break the
/// `index-{volume_id}.db` filename or the `{device}:{storage}` split.
pub fn device_id_for(serial: Option<&str>, location_id: u64) -> String {
    match serial.map(str::trim).filter(|s| !s.is_empty()) {
        Some(serial) => super::ids::mtp_device_id(serial),
        None => super::ids::mtp_device_id(&location_id.to_string()),
    }
}

/// Build the MTP volume id from a device id and storage id:
/// `{device_id}:{storage_id}`. The storage id is numeric and trails, so
/// [`split_volume_id`] recovers both halves even when the device id holds a `:`.
pub fn mtp_volume_id(device_id: &str, storage_id: u32) -> String {
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
pub fn split_volume_id(volume_id: &str) -> Option<(&str, u32)> {
    let (device_id, storage_str) = volume_id.rsplit_once(':')?;
    let storage_id = storage_str.parse::<u32>().ok()?;
    Some((device_id, storage_id))
}

/// The device id half of an MTP volume id (`{device_id}:{storage_id}`), or
/// `None` if the id isn't a well-formed MTP volume id. Convenience over
/// [`split_volume_id`] for callers that only need the device.
pub fn device_id_of_volume(volume_id: &str) -> Option<&str> {
    split_volume_id(volume_id).map(|(device_id, _)| device_id)
}

/// The storage id half of an MTP volume id, or `None` if malformed. Convenience
/// over [`split_volume_id`] for callers that only need the storage.
pub fn storage_id_of_volume(volume_id: &str) -> Option<u32> {
    split_volume_id(volume_id).map(|(_, storage_id)| storage_id)
}

/// Whether `id` looks like an MTP device id (carries the `mtp-` prefix). A cheap
/// shape check; it does NOT prove the device is connected.
pub fn is_mtp_device_id(id: &str) -> bool {
    id.starts_with(MTP_DEVICE_ID_PREFIX)
}

/// Whether `volume_id` is a well-formed MTP volume id: an `mtp-`-prefixed device
/// id plus a numeric storage tail. Shape-only (doesn't prove the volume exists).
pub fn is_mtp_volume_id(volume_id: &str) -> bool {
    split_volume_id(volume_id).is_some_and(|(device_id, _)| is_mtp_device_id(device_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── device_id_for: serial preferred, location fallback ────────────────

    #[test]
    fn prefixes_every_device_id() {
        assert!(device_id_for(Some("ABC123"), 336_592_896).starts_with(MTP_DEVICE_ID_PREFIX));
        assert!(device_id_for(None, 336_592_896).starts_with(MTP_DEVICE_ID_PREFIX));
    }

    #[test]
    fn prefix_matches_the_id_funnel() {
        // `MTP_DEVICE_ID_PREFIX` and the funnel's scheme are two spellings of one
        // fact; a drift would make `is_mtp_device_id` blind to real MTP ids.
        assert!(super::super::ids::mtp_device_id("anything").starts_with(MTP_DEVICE_ID_PREFIX));
    }

    #[test]
    fn prefers_serial_when_present() {
        // Serial beats topology: the same device on two ports keeps one id, so its
        // index survives a replug (asserted in full below).
        assert_eq!(device_id_for(Some("ABC123"), 336_592_896), device_id_for(Some("ABC123"), 1));
        assert_ne!(device_id_for(Some("ABC123"), 42), device_id_for(None, 42));
    }

    #[test]
    fn distinct_serials_and_locations_get_distinct_ids() {
        assert_ne!(device_id_for(Some("ABC123"), 0), device_id_for(Some("ABC124"), 0));
        assert_ne!(device_id_for(None, 336_592_896), device_id_for(None, 336_592_897));
    }

    #[test]
    fn treats_empty_or_whitespace_serial_as_absent() {
        // A device that reports an empty/blank serial must fall back to the
        // topology id rather than to one degenerate id shared by every such device.
        assert_eq!(device_id_for(Some(""), 42), device_id_for(None, 42));
        assert_eq!(device_id_for(Some("   "), 42), device_id_for(None, 42));
        assert_ne!(device_id_for(Some(""), 42), device_id_for(Some(""), 43));
    }

    #[test]
    fn a_punctuated_serial_cannot_break_the_id() {
        // Serials arrive from the device, so they're hostile input: a `:` would
        // shift the storage split, and a `/` or `.` would break `index-{id}.db`.
        // The funnel encodes them out of existence.
        let id = device_id_for(Some("AA:BB/CC.DD"), 7);
        assert!(id.chars().all(|c| c.is_alphanumeric() || c == '-'), "got: {id}");
        assert_ne!(id, device_id_for(Some("AABBCCDD"), 7));
    }

    // ── split_volume_id: recovering both halves ───────────────────────────

    #[test]
    fn splits_a_plain_location_volume_id() {
        let device_id = device_id_for(None, 336_592_896);
        let volume_id = mtp_volume_id(&device_id, 65537);
        assert_eq!(split_volume_id(&volume_id), Some((device_id.as_str(), 65537)));
    }

    #[test]
    fn splits_a_serial_volume_id() {
        let device_id = device_id_for(Some("ABC123"), 0);
        let volume_id = mtp_volume_id(&device_id, 65537);
        assert_eq!(split_volume_id(&volume_id), Some((device_id.as_str(), 65537)));
    }

    #[test]
    fn splits_from_the_right_so_a_stray_colon_cannot_shift_it() {
        // Defense in depth: the funnel already keeps `:` out of a device id, and
        // rsplit_once holds even for an id that somehow carries one (the storage id
        // is always the trailing numeric component). A naive `split(':').nth(1)`
        // would return "BB" here and fail the u32 parse.
        assert_eq!(split_volume_id("mtp-AA:BB:CC:65537"), Some(("mtp-AA:BB:CC", 65537)));
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
        let device_id = device_id_for(Some("ABC"), 0);
        assert!(is_mtp_device_id(&device_id));
        assert!(!is_mtp_device_id("root"));
        assert!(!is_mtp_device_id(&super::super::ids::smb_volume_id("nas", 445, "share")));

        assert!(is_mtp_volume_id(&mtp_volume_id(&device_id, 65537)));
        assert!(is_mtp_volume_id("mtp-AA:BB:CC:65537"));
        // An SMB volume id carries no `:` at all, so it can't even split; the
        // `mtp-` prefix check excludes it a second time.
        assert!(!is_mtp_volume_id(&super::super::ids::smb_volume_id("host", 445, "1234")));
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
