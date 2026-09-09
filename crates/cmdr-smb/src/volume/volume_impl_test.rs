//! The `Volume` surface a session-free share can answer: re-rooting and every
//! capability flag. Path translation has its own cell file (`paths_test.rs`).
//!
//! What each flag COSTS when answered wrong is in the doc comments; several are
//! silent failures, which is why they're pinned one by one.

use super::*;
use crate::volume::test_support::*;
use cmdr_fs::volume::SignInShape;
use cmdr_fs::volume::host::VolumeHost;
use std::sync::Arc;

#[test]
fn can_watch_listings_returns_false() {
    let vol = make_test_volume();
    assert!(!vol.can_watch_listings());
}

#[test]
fn name_returns_share_name() {
    let vol = make_test_volume();
    assert_eq!(vol.name(), "TestShare");
}

#[test]
fn root_returns_mount_path() {
    let vol = make_test_volume();
    assert_eq!(vol.root(), Path::new("/Volumes/TestShare"));
}

/// macOS mounts one share at several roots (`/Volumes/naspi` AND
/// `/Volumes/naspi-1`), which all derive one volume ID, so the registry has to be
/// able to move the ID to whichever mount is still alive. A direct `SmbVolume`
/// re-roots because its transport doesn't ride the mount at all: the new instance
/// addresses the new root over the SAME smb2 session, with no re-auth and nothing
/// to rebuild.
#[test]
fn rerooting_addresses_the_new_mount_over_the_same_session() {
    let vol = make_test_volume();

    let promoted = vol
        .rerooted(Path::new("/Volumes/TestShare-1"))
        .expect("a direct SMB share re-roots: its session doesn't ride the OS mount");

    assert_eq!(promoted.root(), Path::new("/Volumes/TestShare-1"));
    assert_eq!(promoted.name(), "TestShare", "the display name survives a promotion");
    assert_eq!(
        vol.root(),
        Path::new("/Volumes/TestShare"),
        "the replaced instance keeps serving in-flight work at the root it was handed"
    );

    let promoted_smb = promoted
        .as_any()
        .downcast_ref::<SmbVolume>()
        .expect("a rerooted SMB volume is still an SmbVolume");
    assert_eq!(promoted_smb.volume_id(), vol.volume_id(), "same share, same ID");

    // The same session, not a copy of it: a connection-state change on one
    // instance is visible through the other. That's what "no session churn"
    // means, and it's why a running copy survives a promotion.
    vol.inner.transition_to_direct();
    assert_eq!(
        promoted_smb.session_state(),
        ConnectionState::Direct,
        "both instances read one live session's state"
    );
}

/// Every path a rerooted instance translates or hands out is anchored to its NEW
/// mount, and the old mount is no longer one of its paths: the whole point of the
/// promotion is that Cmdr stops publishing `/Volumes/naspi/…` once that mount is
/// gone.
#[test]
fn a_rerooted_share_translates_paths_under_its_new_mount() {
    let vol = make_test_volume();
    let promoted = vol
        .rerooted(Path::new("/Volumes/TestShare-1"))
        .expect("a direct SMB share re-roots");
    let promoted_smb = promoted
        .as_any()
        .downcast_ref::<SmbVolume>()
        .expect("a rerooted SMB volume is still an SmbVolume");

    assert_eq!(
        promoted_smb
            .to_smb_path(Path::new("/Volumes/TestShare-1/Documents/report.pdf"))
            .expect("a path inside the new mount"),
        "Documents/report.pdf"
    );
    assert!(
        matches!(
            promoted_smb.to_smb_path(Path::new("/Volumes/TestShare/Documents/report.pdf")),
            Err(VolumeError::NotFound(_))
        ),
        "the old mount isn't this instance's root any more"
    );
    assert_eq!(
        promoted_smb.to_display_path("Documents/report.pdf"),
        "/Volumes/TestShare-1/Documents/report.pdf"
    );
}

#[test]
fn local_path_returns_none() {
    let vol = make_test_volume();
    assert!(vol.local_path().is_none());
}

#[test]
fn supports_export_returns_true() {
    let vol = make_test_volume();
    assert!(vol.supports_export());
}

/// The opt-in that makes a background SMB copy stand aside for navigation. A copy
/// and the pane's listings share one SMB session, so without this the transfer
/// never parks and the share stays sluggish for the whole copy.
#[test]
fn supports_foreground_yield_is_on() {
    let vol = make_test_volume();
    assert!(vol.supports_foreground_yield());
}

/// The two locality questions an SMB share answers DIFFERENTLY, which is the
/// whole reason `paths_are_os_visible` exists as its own capability.
///
/// Cmdr's own I/O never goes through `std::fs` here (it rides the smb2 session),
/// but the share stays OS-mounted alongside it, so other apps CAN open the paths
/// this volume hands out. The macOS drag-out path reads the second answer: while
/// it read the first, dragging from an SMB pane published file promises only,
/// and every drop target except Finder (a browser upload widget, a mail
/// composer) rejected the drop.
#[test]
fn os_mounted_share_is_os_visible_even_though_cmdr_avoids_std_fs() {
    let vol = make_test_volume();
    assert!(
        !vol.supports_local_fs_access(),
        "Cmdr's own reads go over smb2, not std::fs"
    );
    assert!(
        vol.paths_are_os_visible(),
        "the sneaky mount is what makes an SMB path droppable into another app"
    );
}

/// …and the answer stops being `true` the moment the registry proves this
/// instance's mount is gone with no live sibling to move to.
///
/// Cmdr keeps browsing the share (its I/O rides smb2, which never touched the
/// mount), so nothing looks broken — but a `file://` URL under a mount that isn't
/// there opens nowhere, and a drag into Mail or a browser upload widget silently
/// does nothing. The volume can't find this out on its own: probing a wedged mount
/// blocks 30–120 s, so the registry pushes what it knows.
#[test]
fn a_share_whose_mount_is_gone_stops_claiming_its_paths_are_os_visible() {
    let vol = make_test_volume();
    assert!(vol.paths_are_os_visible(), "the mount is there to begin with");

    vol.note_root_mount_gone();

    assert!(
        !vol.paths_are_os_visible(),
        "no mount, no `file://` URL another app can open"
    );
    assert!(
        !vol.supports_local_fs_access(),
        "still the separate question it always was: Cmdr's own reads go over smb2"
    );
}

/// The flag belongs to ONE mount root, so a promotion onto a live mount answers
/// honestly again: the whole point of re-rooting is that the share's paths work.
#[test]
fn a_reroot_onto_a_live_mount_is_os_visible_again() {
    let vol = make_test_volume();
    vol.note_root_mount_gone();

    let promoted = vol
        .rerooted(Path::new("/Volumes/TestShare-1"))
        .expect("a direct SMB share re-roots");

    assert!(
        promoted.paths_are_os_visible(),
        "the new root is a live mount until something proves otherwise"
    );
    assert!(
        !vol.paths_are_os_visible(),
        "the instance still anchored to the dead mount keeps its answer"
    );
}

/// The UPLOAD counterpart: an SMB share also opts into the DESTINATION-side yield,
/// so writing to it stands aside for navigation on the same share. SMB writes are
/// discrete WRITE chunks with no lease, so a bounded park between them is safe.
#[test]
fn supports_foreground_yield_as_destination_is_on() {
    let vol = make_test_volume();
    assert!(vol.supports_foreground_yield_as_destination());
}

/// THE GATE on the transfer watchdog's one aggressive action, from the backend
/// side: `SmbVolume` reports NO liveness verdict, so the watchdog reports a stall
/// and never ends anyone's wait.
///
/// `smb2` 0.16.0 has an ECHO keepalive and it still doesn't change this. A missed
/// probe is deliberately not a death verdict — a busy NAS drops probes precisely
/// while it writes — and the crate's one sound verdict
/// (`Error::ServerUnresponsive`) is an error handed to the caller AFTER the
/// connection has been torn down and every waiter failed, which the per-file
/// retry already covers. ❌ Don't answer `Dead` here from `keepalive_failures`, a
/// slow response, or `is_disconnected()`; read
/// `write_operations/transfer/DETAILS.md` § "The watchdog ACTS" first, which says
/// what `smb2` would have to expose for this to become `Some`.
#[test]
fn connection_liveness_reports_no_verdict() {
    let vol = make_test_volume();
    assert!(
        vol.connection_liveness().is_none(),
        "SMB has no sound dead-vs-slow signal to answer with; see the doc above before changing this"
    );
}

/// …and the probe behind it asks about THIS share's own volume id, which is what
/// scopes the yield: navigating the volume being copied from parks the copy,
/// navigating anything else leaves it at full speed.
///
/// `foreground_yield`'s own tests pin the scoping rule against the seam; what
/// this pins is that `SmbVolume` asks it the right question.
#[tokio::test]
async fn foreground_pending_tracks_navigation_on_this_share_only() {
    use cmdr_fs::volume::host::activity::BusyVolumes;

    let volume_id = "smb-foreground-scope";
    let quiet = make_test_volume_with(volume_id, VolumeHost::detached());
    assert!(!quiet.foreground_pending().await, "nothing browsed yet");

    let elsewhere = VolumeHost::builder()
        .activity(Arc::new(BusyVolumes::new().is_busy("some-other-volume")))
        .build();
    let vol = make_test_volume_with(volume_id, elsewhere);
    assert!(
        !vol.foreground_pending().await,
        "browsing another volume must not park a copy off this share"
    );

    let here = VolumeHost::builder()
        .activity(Arc::new(BusyVolumes::new().is_busy(volume_id)))
        .build();
    let vol = make_test_volume_with(volume_id, here);
    assert!(vol.foreground_pending().await, "browsing this share parks the copy");
}

// ── Copy concurrency against the credit window ─────────────────

/// A `BackendSettings` that answers one fixed number, so a test can pin what the
/// user asked for and assert only what the credit window did to it.
struct FixedConcurrency(usize);

impl cmdr_fs::volume::host::settings::BackendSettings for FixedConcurrency {
    fn max_concurrent_operations(&self, _backend: cmdr_fs::volume::host::settings::BackendName) -> usize {
        self.0
    }
}

/// A volume whose setting says `requested` and whose connection last measured
/// room for `capacity` concurrent copy requests (`0` = never measured).
fn volume_with_credit_capacity(requested: usize, capacity: usize) -> SmbVolume {
    let host = VolumeHost::builder()
        .settings(Arc::new(FixedConcurrency(requested)))
        .build();
    let vol = make_test_volume_with("smb-credit-capacity", host);
    vol.inner.credit_copy_capacity.store(capacity, Ordering::Relaxed);
    vol
}

/// Until a session has been cloned there's no measurement, and a made-up cap
/// would be worse than none: the user's setting stands.
#[test]
fn concurrency_is_the_setting_while_the_window_is_unmeasured() {
    assert_eq!(volume_with_credit_capacity(10, 0).max_concurrent_ops(), 10);
}

/// The whole point of the cap: a window that can't carry ten concurrent copies
/// must not be handed ten, or the extra slots park on credits while reporting no
/// progress (a 300 GB SMB-to-SMB copy stalled exactly this way).
#[test]
fn concurrency_drops_to_what_the_credit_window_carries() {
    assert_eq!(volume_with_credit_capacity(10, 3).max_concurrent_ops(), 3);
}

/// It only ever LOWERS the setting. A roomy window doesn't get to overrule the
/// number the user chose, which is also a statement about the server's appetite,
/// not just about credits.
#[test]
fn a_roomy_window_never_raises_the_users_setting() {
    assert_eq!(volume_with_credit_capacity(10, 64).max_concurrent_ops(), 10);
}

/// One slot is the floor on both sides. A copy engine handed `0` slots does
/// nothing at all, which is a hang rather than a slow copy, so a window with
/// room for a single request still yields a working (serial) copy.
#[test]
fn concurrency_never_falls_below_one() {
    assert_eq!(volume_with_credit_capacity(10, 1).max_concurrent_ops(), 1);
    assert_eq!(volume_with_credit_capacity(1, 8).max_concurrent_ops(), 1);
}

/// ❗ **The share is the identity, so the username is a FIELD on it.** SMB's
/// `reconnect_with_credentials` accepts a new username and rewrites its params,
/// which is how "sign in as someone else" works on a share; the sheet only offers
/// that where the backend says so, and the default `Password` shape would render
/// the username read-only and quietly take the affordance away.
#[test]
fn a_share_asks_for_a_username_alongside_the_password() {
    let vol = make_test_volume();

    assert_eq!(
        vol.sign_in_prompt(),
        SignInShape::UsernamePassword { guest_allowed: true },
        "the test double holds a guest session, which is the only guest fact a volume knows",
    );
}

/// A share opened with real credentials says nothing about guests: it never
/// tried. The sheet then shows two fields and no guest button, which is the
/// honest rendering of "we don't know".
#[test]
fn a_share_opened_with_credentials_offers_no_guest_option() {
    let vol = make_test_volume();
    {
        let mut params = vol.inner.params.try_write().expect("nothing else holds the params");
        params.username = "ada".to_string();
        params.password = "hunter2".to_string();
    }

    assert_eq!(
        vol.sign_in_prompt(),
        SignInShape::UsernamePassword { guest_allowed: false }
    );
}

// ── Promoting a mount anchored inside the share ───────────────────────────────

/// A recorded root promotes to ITS anchor, not the one the current instance
/// happens to hold: two mounts of one share need not sit at the same place in it.
#[test]
fn a_promotion_uses_the_anchor_recorded_for_the_target_root() {
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    vol.note_mount_root(MountAnchor::at_share_root("/Volumes/SYSVOL"));

    let promoted = vol.rerooted(Path::new("/Volumes/SYSVOL")).expect("a recorded root");
    let promoted = promoted
        .as_any()
        .downcast_ref::<SmbVolume>()
        .expect("still an SmbVolume");

    assert_eq!(promoted.share_root, "", "the target root sits at the share root");
    assert_eq!(
        promoted
            .to_smb_path(Path::new("/Volumes/SYSVOL/lgs-net.com/Policies"))
            .expect("a path inside the new mount"),
        "lgs-net.com/Policies",
        "carrying the old anchor over would have asked for lgs-net.com/lgs-net.com/Policies"
    );
}

/// An anchored instance refuses a root nobody placed, rather than joining its own
/// anchor onto a mount that may not share it: that addresses a real path on the
/// share nobody asked for. The registry reads `None` as "can't re-root".
#[test]
fn an_anchored_share_refuses_to_promote_to_an_unrecorded_root() {
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert!(vol.rerooted(Path::new("/Volumes/SomewhereElse")).is_none());
}

/// The ordinary share keeps promoting to any root the registry hands it. Roots
/// reach the registry from paths that never touch the SMB upgrade, so this is the
/// common case, and an unanchored mount has no anchor to get wrong.
#[test]
fn an_unanchored_share_still_promotes_to_an_unrecorded_root() {
    let vol = make_test_volume();
    let promoted = vol
        .rerooted(Path::new("/Volumes/TestShare-1"))
        .expect("an unanchored share re-roots to any root");
    assert_eq!(
        promoted
            .as_any()
            .downcast_ref::<SmbVolume>()
            .expect("still an SmbVolume")
            .share_root,
        ""
    );
}

/// A reconnect builds a new instance over a new session while the registry keeps
/// the roots it already had. Without inheriting them the successor would refuse a
/// promotion its predecessor would have allowed.
#[test]
fn a_successor_inherits_the_mount_roots_its_predecessor_knew() {
    let predecessor = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    predecessor.note_mount_root(MountAnchor::at_share_root("/Volumes/SYSVOL"));

    let successor = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert!(
        successor.rerooted(Path::new("/Volumes/SYSVOL")).is_none(),
        "nothing told the successor about that root yet"
    );

    successor.exchange_mount_roots_with(&predecessor);

    let promoted = successor.rerooted(Path::new("/Volumes/SYSVOL")).expect("now recorded");
    assert_eq!(
        promoted
            .as_any()
            .downcast_ref::<SmbVolume>()
            .expect("still an SmbVolume")
            .share_root,
        ""
    );
}

/// The other direction of the same exchange. When the registry keeps the
/// incumbent, the newcomer is dropped and its root would go with it, leaving the
/// surviving volume unable to promote to a mount that is right there.
#[test]
fn the_incumbent_learns_where_the_newcomer_sits_inside_the_share() {
    let incumbent = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    let newcomer = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL-1/lgs-net.com");
    assert!(
        incumbent.rerooted(Path::new("/Volumes/SYSVOL-1/lgs-net.com")).is_none(),
        "nothing told the incumbent about the second mount yet"
    );

    newcomer.exchange_mount_roots_with(&incumbent);

    let promoted = incumbent
        .rerooted(Path::new("/Volumes/SYSVOL-1/lgs-net.com"))
        .expect("the newcomer's own root reached the incumbent");
    assert_eq!(
        promoted
            .as_any()
            .downcast_ref::<SmbVolume>()
            .expect("still an SmbVolume")
            .share_root,
        "lgs-net.com",
        "and it arrived with the anchor the newcomer proved, not an assumed one"
    );
}
