//! Tests for the NetFS mount path (`mount.rs`).
//!
//! Split out under the directory's `*_test.rs` convention: the module's own surface
//! is small and its tests are not (three drive real Docker Samba containers, two of
//! those through a real macOS kernel mount).

use super::*;

/// `CFURLCreateWithString` parses, it does not escape: it returns NULL for any
/// string that isn't already a valid RFC 3986 URL, so a share whose name carries
/// a non-ASCII byte used to fail before a single packet went out. Every share on
/// the `unicode` fixture host reproduces it.
#[test]
fn mount_url_percent_encodes_non_ascii_share_names() {
    assert_eq!(
        build_smb_mount_url("localhost", "café", 11484),
        "smb://localhost:11484/caf%C3%A9"
    );
    assert_eq!(
        build_smb_mount_url("localhost", "公開", 11484),
        "smb://localhost:11484/%E5%85%AC%E9%96%8B"
    );
    assert_eq!(
        build_smb_mount_url("localhost", "文档", 445),
        "smb://localhost/%E6%96%87%E6%A1%A3"
    );
}

/// Reserved characters are legal in an SMB share name and must survive the trip
/// as data. `%` matters most: unescaped, `100%` reads as a truncated escape and
/// the URL is rejected; `#` and `?` would silently cut the share name short.
#[test]
fn mount_url_percent_encodes_reserved_characters_in_share_names() {
    assert_eq!(build_smb_mount_url("nas", "100%", 445), "smb://nas/100%25");
    assert_eq!(build_smb_mount_url("nas", "Q&A #1", 445), "smb://nas/Q%26A%20%231");
    assert_eq!(build_smb_mount_url("nas", "who?", 445), "smb://nas/who%3F");
    // A share name can't contain a slash, but if one ever reached us it must not
    // be able to graft an extra path segment onto the URL.
    assert_eq!(build_smb_mount_url("nas", "a/b", 445), "smb://nas/a%2Fb");
}

/// The scheme, the `//` authority marker, the port colon, and the share separator
/// are structure, not data: a blanket escape of the whole URL string would eat them.
#[test]
fn mount_url_leaves_scheme_and_separators_intact() {
    assert_eq!(
        build_smb_mount_url("192.168.1.111", "naspi", 445),
        "smb://192.168.1.111/naspi"
    );
    assert_eq!(
        build_smb_mount_url("naspolya.local", "naspi", 1445),
        "smb://naspolya.local:1445/naspi"
    );
}

/// macOS hands out NFD (decomposed) strings while SMB servers store and answer
/// with NFC, so the same visible name is two different byte strings and two
/// different escapes. We normalize to NFC for the same reason
/// `cmdr_smb::volume::paths` does on every path it sends.
#[test]
fn mount_url_normalizes_decomposed_names_to_nfc() {
    // "café" spelled `e` + U+0301 COMBINING ACUTE ACCENT.
    let decomposed = "cafe\u{301}";
    assert_eq!(
        build_smb_mount_url("localhost", decomposed, 11484),
        build_smb_mount_url("localhost", "café", 11484),
        "NFD and NFC spell the same share; both must produce the URL the server answers to"
    );
    // Same for the server half: an mDNS name can arrive decomposed too.
    assert_eq!(
        build_smb_mount_url("Zu\u{308}rich.local", "public", 445),
        build_smb_mount_url("Zürich.local", "public", 445)
    );
    assert_eq!(
        build_smb_mount_url("Zürich.local", "public", 445),
        "smb://Z%C3%BCrich.local/public"
    );
}

/// An IPv6 literal is the one host shape that must NOT be escaped: it needs
/// brackets so its colons can't be read as the port separator. mDNS hands us one
/// whenever a host advertises no IPv4 address (`extract_preferred_ip`).
#[test]
fn mount_url_brackets_ipv6_literals() {
    assert_eq!(build_smb_mount_url("fe80::1", "public", 445), "smb://[fe80::1]/public");
    assert_eq!(
        build_smb_mount_url("fe80::1", "public", 11484),
        "smb://[fe80::1]:11484/public"
    );
    // Already bracketed by the caller: don't double-wrap.
    assert_eq!(
        build_smb_mount_url("[fe80::1]", "public", 445),
        "smb://[fe80::1]/public"
    );
}

#[test]
fn test_error_from_code() {
    let err = error_from_code(USER_CANCELLED_ERR, "test", "server");
    match err {
        MountError::Cancelled { .. } => (),
        _ => panic!("Expected Cancelled error"),
    }

    let err = error_from_code(ENOENT, "Share1", "Server1");
    match err {
        MountError::ShareNotFound { message } => {
            // allowed-error-string-match: testing Display content of MountError::ShareNotFound message field
            assert!(message.contains("Share1"));
            // allowed-error-string-match: testing Display content of MountError::ShareNotFound message field
            assert!(message.contains("Server1"));
        }
        _ => panic!("Expected ShareNotFound error"),
    }

    let err = error_from_code(EAUTH, "test", "server");
    match err {
        MountError::AuthFailed { .. } => (),
        _ => panic!("Expected AuthFailed error"),
    }

    let err = error_from_code(EHOSTUNREACH, "test", "server");
    match err {
        MountError::HostUnreachable { .. } => (),
        _ => panic!("Expected HostUnreachable error"),
    }
}

/// NetAuth error codes (NetAuthAgent, documented in `<NetFS/NetFS.h>`) must map to
/// typed errors, not the opaque `ProtocolError` catch-all. -6600 is what
/// `NetFSMountURLSync` returns when authentication fails (observed in the wild with
/// a guest mount against a creds-required NAS); routing it to `AuthFailed` is what
/// lets the frontend offer the login form instead of a dead-end error pane.
#[test]
fn test_netauth_error_codes() {
    let err = error_from_code(-6600, "naspi", "naspolya");
    assert!(
        matches!(err, MountError::AuthFailed { .. }),
        "kNetAuthErrorInternal (-6600) should be AuthFailed, got {:?}",
        err
    );

    let err = error_from_code(-6004, "naspi", "naspolya");
    assert!(
        matches!(err, MountError::AuthRequired { .. }),
        "kNetAuthErrorGuestNotSupported (-6004) should be AuthRequired, got {:?}",
        err
    );

    let err = error_from_code(-6003, "naspi", "naspolya");
    assert!(
        matches!(err, MountError::ShareNotFound { .. }),
        "kNetAuthErrorNoSharesAvailable (-6003) should be ShareNotFound, got {:?}",
        err
    );

    // kNetAuthErrorMountFailed means auth SUCCEEDED but the mount step failed, so it
    // must NOT map to an auth-class error (that would loop the user into a pointless
    // login form). It stays a ProtocolError, just with a readable message.
    let err = error_from_code(-6602, "naspi", "naspolya");
    assert!(
        matches!(err, MountError::ProtocolError { .. }),
        "kNetAuthErrorMountFailed (-6602) should stay ProtocolError, got {:?}",
        err
    );
}

/// `UIOption = NoUI` must be set on EVERY mount, regardless of guest/credentialed
/// mode. Without it, NetFS hands auth failures to NetAuthAgent, which pops a system
/// dialog ("You entered an invalid username or password...") on top of Cmdr and then
/// returns `kNetAuthErrorInternal`. Cmdr owns all auth UI.
#[test]
fn test_open_options_always_suppress_system_ui() {
    for (guest, force_new_session) in [(false, false), (true, false), (false, true), (true, true)] {
        let entries = open_option_entries(guest, force_new_session);
        assert!(
            entries.contains(&("UIOption", OpenOptionValue::Str("NoUI"))),
            "UIOption=NoUI missing for guest={guest}, force_new_session={force_new_session}: {entries:?}"
        );
        assert_eq!(
            entries.iter().any(|(key, _)| *key == "Guest"),
            guest,
            "Guest key presence should match guest={guest}"
        );
        assert_eq!(
            entries.iter().any(|(key, _)| *key == "ForceNewSession"),
            force_new_session,
            "ForceNewSession key presence should match force_new_session={force_new_session}"
        );
    }
}

#[test]
fn test_timeout_constant() {
    // Verify default timeout is reasonable (10-60 seconds)
    const { assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000) };
    const { assert!(DEFAULT_MOUNT_TIMEOUT_MS <= 60_000) };
}

/// Regression test for the macOS NetFS guest-mount credential dialog.
///
/// Asserts a guest mount completes within a tight wall-clock budget. A
/// blocking kernel `smbfs` prompt waits for user input indefinitely, so a
/// sub-budget completion is the proxy for "no dialog appeared." Gated to
/// macOS because Linux uses gvfs, which has neither the dialog nor this
/// mount path.
///
/// We don't add a paired auth-success / auth-failure test here because
/// NetFS caches SMB sessions across calls — once `testuser`+`testpass`
/// authenticates once, subsequent calls (even with wrong creds) ride the
/// cached session, so a tight harness can't reliably distinguish "creds
/// passed correctly" from "session reused" without forcibly tearing down
/// the session. The guest path is what regressed in real use and is what
/// this test guards. Manual end-to-end coverage for the auth path runs
/// via `pnpm dev` against the same Docker containers.
#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_mount_guest_no_dialog() {
    use std::time::{Duration, Instant};

    let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480);
    // Use `localhost` rather than `127.0.0.1`: NetFS itself handles either,
    // but the wider SMB test harness uses `localhost` to dodge the smbutil
    // loopback quirk on non-standard ports.
    let host = "localhost".to_string();

    // Pre-clean any stale mount from a previous run so we exercise the
    // real first-mount path (the one that pops the dialog when broken).
    let _ = std::process::Command::new("diskutil")
        .args(["unmount", "force", "/Volumes/public"])
        .output();

    // 10 s budget: a real credential dialog blocks the call indefinitely,
    // so this picks up the regression even under cold Docker startup.
    let budget = Duration::from_secs(10);
    let start = Instant::now();
    let result = mount_share(host.clone(), "public".to_string(), None, None, port, Some(8_000)).await;
    let elapsed = start.elapsed();

    // Always try to unmount so a successful mount doesn't linger between runs.
    if let Ok(ref ok) = result {
        let _ = std::process::Command::new("diskutil")
            .args(["unmount", "force", &ok.mount_path])
            .output();
    }

    assert!(
        elapsed < budget,
        "guest mount took {:?} (budget {:?}); a credential dialog probably blocked NetFS",
        elapsed,
        budget
    );
    let mount_result = result.unwrap_or_else(|e| panic!("guest mount against {host}:{port} failed: {e:?}"));
    assert!(
        mount_result.mount_path.starts_with("/Volumes/"),
        "expected /Volumes/* mount path, got {}",
        mount_result.mount_path
    );
}

/// Regression test for the SMB volume-ID-per-mount fix.
///
/// An SMB volume ID must key on `(server, port, share)`, never on the mount
/// path. A path-derived ID gives two shares with the same case-folded name on
/// different servers (a NAS sharing `Public`, a Docker container sharing
/// `public`) one ID, which cross-contaminates `lastUsedPaths` and tab state
/// and surfaces as wrong-case paths flowing into `SmbVolume::list_directory`,
/// producing `STATUS_OBJECT_PATH_NOT_FOUND` from the server.
///
/// Exercises the real OS-mount → `resolve_path_volume_fast` path against the
/// Docker guest container, then asserts the resulting volume ID is SMB-shaped
/// and embeds the port.
#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_volume_id_is_per_mount_not_per_path_shape() {
    use std::time::Duration;

    let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10480);
    let host = "localhost".to_string();

    // Pre-clean to exercise the cold mount path.
    let _ = std::process::Command::new("diskutil")
        .args(["unmount", "force", "/Volumes/public"])
        .output();

    // RARE, SANCTIONED EXCEPTION: a generous 16s connect timeout (double the usual 8s). This is
    // one of only two SMB tests that go through the real macOS NetFS *kernel* mount
    // (`NetFSMountURLSync`); the other ~36 use the userspace `smb2` lib and need no OS mount.
    // NetFS guest-mount RTT depends on external factors we can't optimize away (the kernel mount
    // queue, plus host CPU/lease contention when the full slow-check suite and both e2e lanes run
    // concurrently), so under load the default 8s spuriously timed out. The mount is pure setup
    // here — this test asserts on the resolved volume id, not mount speed (unlike
    // `smb_integration_mount_guest_no_dialog`, whose 8s budget IS the assertion) — so a bigger
    // budget only changes how long a genuinely-hung mount waits before the nextest 30s
    // slow-timeout cap fires. Don't generalize this number to other tests. See docs/testing.md
    // § "Sanctioned slow-test exceptions".
    let mount_result = mount_share(host.clone(), "public".to_string(), None, None, port, Some(16_000))
        .await
        .unwrap_or_else(|e| panic!("guest mount against {host}:{port} failed: {e:?}"));

    // Force-unmount on EVERY exit path — assertions passing, a panic in them,
    // or the settle wait below timing out — so no run leaks the mount into the
    // next (`Drop` runs on unwind).
    struct UnmountOnDrop(String);
    impl Drop for UnmountOnDrop {
        fn drop(&mut self) {
            let _ = std::process::Command::new("diskutil")
                .args(["unmount", "force", &self.0])
                .output();
        }
    }
    let _unmount = UnmountOnDrop(mount_result.mount_path.clone());

    // Wait for NetFS to register the mount so statfs reports the SMB info. A
    // fixed sleep here raced the OS settling and flaked in BOTH debug and
    // release (the magic-timer-wait anti-pattern — see docs/testing.md). We
    // wait for the settled, SMB-shaped id: an early statfs can briefly report
    // the path-shape id (`volumespublic`) before the SMB mount info lands.
    // The ceiling is generous (20s) because NetFS settle time stretches under
    // the parallel load of the full slow-check suite (Linux tests + both e2e
    // lanes running concurrently); the wait returns on the first satisfied
    // poll, so the budget only ever elapses on a genuine failure.
    let mut volume = None;
    crate::test_support::wait_until_async(
        Duration::from_secs(20),
        "resolve_path_volume_fast to report the settled smb- volume id for a fresh SMB mount",
        || match crate::volumes::resolve_path_volume_fast(&mount_result.mount_path) {
            Some(v) if v.id.starts_with("smb-") => {
                volume = Some(v);
                true
            }
            _ => false,
        },
    )
    .await;
    let volume = volume.expect("the satisfied wait stores the resolved volume");

    // A path-shape ID for `/Volumes/public` would be `volumespublic`, the exact
    // value two different shares used to collide on.
    assert_ne!(
        volume.id, "volumespublic",
        "expected SMB-shaped ID, got the path-shape one (regression)"
    );
    assert!(
        volume.id.starts_with("smb-"),
        "expected SMB-shaped ID (smb-...), got {}",
        volume.id
    );
    // The mount's own coordinates, not the path's. Asserted through the funnel
    // rather than against a spelled-out ID, so the shape can change without
    // this test going stale (only the identity it keys on may not).
    assert_eq!(
        volume.id,
        crate::file_system::volume::smb_volume_id(&host, port, "public"),
        "expected the ID keyed on (server, port, share)"
    );
}

/// A share whose name isn't ASCII must mount, and must be found again afterwards.
///
/// The unit tests above pin the URL we build; only NetFS can say whether it
/// ACCEPTS it, and that half is what regressed: `CFURLCreateWithString` returned
/// NULL for the raw UTF-8 string, so `café` and `公開` couldn't be mounted at all
/// while `public` on the same host mounted fine. The `unicode` fixture host is the
/// only Samba container with non-ASCII share names, which is why the Rust
/// integration lane brings it up (`smblease::modeServices`).
///
/// The second assertion is the other half of the same bug: macOS records the
/// mount source ESCAPED (`//…/caf%C3%A9`), so a raw compare against the name the
/// server advertises reports a live mount as missing.
///
/// ONE share, deliberately: this is a real NetFS *kernel* mount, and the CJK
/// cases add another one of those to the lane while asserting the same mechanism
/// (the unit tests already pin their exact URLs byte for byte). `café` is the one
/// that also carries a distinct NFD spelling. The 16 s budget is the same
/// sanctioned exception as `smb_integration_volume_id_is_per_mount_not_per_path_shape`
/// above — see `docs/testing.md` § "Sanctioned slow-test exceptions"; the mount is
/// setup here, not the assertion.
#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_mount_non_ascii_share() {
    let port: u16 = std::env::var("SMB_CONSUMER_UNICODE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10484);
    let host = "localhost".to_string();
    let share = "café";

    // Pre-clean so we exercise the cold mount path, not an EEXIST shortcut.
    let _ = std::process::Command::new("diskutil")
        .args(["unmount", "force", &format!("/Volumes/{share}")])
        .output();

    let result = mount_share(host.clone(), share.to_string(), None, None, port, Some(16_000)).await;

    // Unmount on every exit path, assertion failures included.
    struct UnmountOnDrop(String);
    impl Drop for UnmountOnDrop {
        fn drop(&mut self) {
            let _ = std::process::Command::new("diskutil")
                .args(["unmount", "force", &self.0])
                .output();
        }
    }
    let mount = result.unwrap_or_else(|e| panic!("mounting {share:?} on {host}:{port} failed: {e:?}"));
    let _unmount = UnmountOnDrop(mount.mount_path.clone());

    assert!(
        mount.mount_path.starts_with("/Volumes/"),
        "expected a /Volumes/* mount path for {share:?}, got {}",
        mount.mount_path
    );
    assert_eq!(
        find_mount_path_for_share(&host, share, port).as_deref(),
        Some(mount.mount_path.as_str()),
        "the live mount for {share:?} must be findable under the name the server advertises"
    );
}
