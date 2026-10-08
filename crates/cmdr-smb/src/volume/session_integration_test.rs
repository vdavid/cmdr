//! What the SESSION does, against a real server: the connection gate the
//! fresh-listing oracle reads, the reconnect cycle, the refcounted scan pool,
//! and what a supersede leaves alone.
//!
//! File-level behavior is `integration_test.rs`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::test_support::*;
use super::*;

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_listing_watch_coverage_flips_with_connection() {
    // End-to-end check against a live Docker SMB server: after
    // `connect_smb_volume`, the watcher is spawned and state is Direct,
    // so the oracle gate reports full coverage. After flipping the state to
    // Disconnected (simulating a ConnectionLost event), the gate flips
    // false even though `watcher_cancel` is still set: the contract is
    // "watcher present AND Direct," and a half-broken volume must not be
    // treated as fresh.
    let vol = make_docker_volume().await;
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert_eq!(
        vol.listing_watch_coverage(Path::new("/")),
        WatchCoverage::EveryWriter,
        "expected full coverage on a freshly-connected Docker volume"
    );

    vol.inner.transition_to_disconnected();
    assert_eq!(
        vol.listing_watch_coverage(Path::new("/")),
        WatchCoverage::None,
        "expected no coverage after transitioning to Disconnected"
    );
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_attempt_reconnect_rebuilds_session() {
    // Drives the full reconnect cycle against a real SMB server:
    // 1. Connect, verify Direct.
    // 2. Force-flip to Disconnected (simulating a ConnectionLost event). Drop the underlying client +
    //    tree to mimic a dead session.
    // 3. Verify hot-path ops fail with DeviceDisconnected.
    // 4. Call attempt_reconnect; verify it succeeds and state is Direct.
    // 5. Verify hot-path ops work again.
    let vol = make_docker_volume().await;
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert!(vol.list_directory_impl(Path::new("")).await.is_ok());

    // Simulate "the server hung up": drop the smb2 session and flip state.
    // We don't need to actually break the network; `attempt_reconnect`'s
    // job is to rebuild the session regardless of why state went down.
    {
        let mut client_guard = vol.inner.client.lock().await;
        *client_guard = None;
    }
    {
        let mut tree_guard = vol.inner.tree.write().await;
        *tree_guard = None;
    }
    vol.inner.transition_to_disconnected();
    assert_eq!(vol.session_state(), ConnectionState::Disconnected);

    // Hot-path op should fail: clone_session refuses while Disconnected.
    let result = vol.list_directory_impl(Path::new("")).await;
    assert!(
        matches!(result, Err(VolumeError::DeviceDisconnected(_))),
        "expected DeviceDisconnected before reconnect, got {:?}",
        result
    );

    // Reconnect should rebuild the session and flip back to Direct.
    vol.inner
        .do_attempt_reconnect()
        .await
        .expect("attempt_reconnect should succeed against a live Docker SMB");
    assert_eq!(vol.session_state(), ConnectionState::Direct);

    // And hot-path ops should work again.
    let entries = vol
        .list_directory_impl(Path::new(""))
        .await
        .expect("list_directory should succeed after reconnect");
    assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
}

/// The transfer watchdog asks `connection_liveness` whether to end a wait, so the
/// answer has to come from the session a copy would ride NOW. After a rebuild the
/// old connection is torn down; a reading left pointing at it would say
/// `Disconnected` about a share that's working, or, worse, carry a dead session's
/// verdict over to a live one.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_reconnect_repoints_the_liveness_reading() {
    let vol = make_docker_volume().await;
    let old = vol
        .inner
        .live_connection
        .current()
        .expect("a connected share has a reading");
    assert!(
        vol.connection_bytes_received().is_some_and(|b| b > 0),
        "the handshake came in"
    );

    {
        let mut client_guard = vol.inner.client.lock().await;
        *client_guard = None;
    }
    vol.inner.transition_to_disconnected();
    vol.inner
        .do_attempt_reconnect()
        .await
        .expect("attempt_reconnect should succeed against a live Docker SMB");
    old.mark_dead();

    let current = vol
        .inner
        .live_connection
        .current()
        .expect("the rebuilt session is installed");
    assert_eq!(old.liveness(), smb2::Liveness::Disconnected);
    assert_ne!(
        current.liveness(),
        smb2::Liveness::Disconnected,
        "the reading must follow the rebuilt session, not the one it replaced"
    );
    vol.list_directory_impl(Path::new(""))
        .await
        .expect("the rebuilt session works");
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_attempt_reconnect_noop_when_already_direct() {
    // Call reconnect against a live, healthy session. Should be a fast no-op
    // (no extra round-trip to the server).
    let vol = make_docker_volume().await;
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    let start = std::time::Instant::now();
    vol.inner.do_attempt_reconnect().await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    // No-op should be effectively instant. Any real session build would
    // take tens of ms minimum even against localhost. Pad the bound for
    // CI noise.
    assert!(
        elapsed < Duration::from_millis(50),
        "noop reconnect took {:?}; expected <50ms",
        elapsed
    );
}

/// The scan-connection pool opens against a live server, serves a scan listing,
/// and closes cleanly. Asserts the internals the server-free `scan_pool::tests`
/// can't reach: the pool is installed after `open_scan_pool`, a
/// `list_directory_for_scan` through it returns the directory's contents, and
/// `close_scan_pool` tears it back down (falling back to the main session).
///
/// Lists a UNIQUE seeded subdirectory, never the shared `public` root, whose
/// entry count races with the many other tests mutating it in parallel.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_pool_opens_lists_and_closes() {
    let vol = make_docker_volume().await;

    // Seed a private directory with two known files, isolated from parallel tests.
    let dir = share_path(&test_dir_name());
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.expect("create test dir");
    vol.create_file(Path::new(&format!("{dir}/a.txt")), b"hello")
        .await
        .expect("create a.txt");
    vol.create_file(Path::new(&format!("{dir}/b.txt")), b"hi")
        .await
        .expect("create b.txt");

    let names = |mut entries: Vec<FileEntry>| -> Vec<String> {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries.into_iter().map(|e| e.name).collect::<Vec<_>>()
    };

    assert!(vol.inner.scan_pool.read().await.is_none(), "no pool before a scan");
    vol.open_scan_pool().await;
    assert!(
        vol.inner.scan_pool.read().await.is_some(),
        "open_scan_pool installs a pool on a connected volume"
    );

    // A scan listing of the seeded dir goes through the pool and returns exactly
    // the two files, matching the main-session listing. Drives the pool acquire +
    // listing path end to end.
    let via_pool = vol
        .list_directory_for_scan_impl(Path::new(&dir))
        .await
        .expect("listing the seeded dir through the pool should succeed");
    let via_main = vol
        .list_directory_impl(Path::new(&dir))
        .await
        .expect("listing the seeded dir through the main session should succeed");
    assert_eq!(names(via_pool.clone()), vec!["a.txt", "b.txt"], "pool sees both files");
    assert_eq!(
        names(via_pool),
        names(via_main),
        "pool listing matches the main session"
    );

    vol.close_scan_pool().await;
    assert!(
        vol.inner.scan_pool.read().await.is_none(),
        "close_scan_pool tears the pool back down"
    );

    // With the pool closed, a scan listing falls back to the main session and
    // still returns the same contents.
    let via_fallback = vol
        .list_directory_for_scan_impl(Path::new(&dir))
        .await
        .expect("scan listing falls back to the main session once the pool is closed");
    assert_eq!(
        names(via_fallback),
        vec!["a.txt", "b.txt"],
        "fallback still lists the files"
    );

    ensure_clean(&vol, &dir).await;
}

/// The pool is REFCOUNTED, because two background users overlap in production: an
/// index rescan kicks while a media-enrichment pass is running. The pool must
/// survive the first `end_scan_session` and close only on the last, or one user's
/// end tears the connections out from under the other mid-flight.
///
/// Needs a live server: `open_scan_pool` only installs a pool on a connected
/// volume, so a session-free volume would pass this vacuously.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_scan_session_pair_shares_one_pool() {
    let vol = make_docker_volume().await;

    vol.begin_scan_session().await;
    vol.begin_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_some(),
        "the pooled connections come up with the scan session"
    );

    vol.end_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_some(),
        "ending one of two scan sessions must not tear the pool down"
    );

    vol.end_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_none(),
        "the last scan session's end closes the pool"
    );
}

/// An unmatched end (a pass racing unmount teardown) saturates at zero instead of
/// underflowing into a pool that can never close again.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_unmatched_scan_session_end_cannot_wedge_the_pool_open() {
    let vol = make_docker_volume().await;

    vol.end_scan_session().await;

    vol.begin_scan_session().await;
    assert!(vol.inner.scan_pool.read().await.is_some(), "the pool still opens");
    vol.end_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_none(),
        "one begin still takes exactly one end, however many stray ends came before"
    );
}

// ── Supersede (an upgrade replacing a live volume) ───────────────

/// The lifecycle invariant behind the SMB upgrade swap, against a real server:
/// retiring a volume must leave its session alive for whoever still holds it.
///
/// A running copy holds `Arc<dyn Volume>` clones of its source and destination
/// (`volume/copy.rs`) for the whole transfer. A redundant upgrade replacing the
/// volume mid-copy used to call `on_unmount` on the predecessor, dropping the
/// smb2 session under the copy and killing it with `DeviceDisconnected` on a
/// connection that was still healthy. `on_superseded` retires the id, not the
/// session.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_superseded_volume_still_serves_its_holders() {
    let vol = Arc::new(make_docker_volume().await);
    assert_eq!(vol.session_state(), ConnectionState::Direct);

    // What an in-flight operation holds across the swap.
    let held = Arc::clone(&vol);
    assert!(
        held.list_directory_impl(Path::new("")).await.is_ok(),
        "sanity: the session works before the swap"
    );

    vol.on_superseded();

    assert!(
        held.list_directory_impl(Path::new("")).await.is_ok(),
        "a superseded volume must keep serving its holders: an upgrade is not a disconnect"
    );
    assert_eq!(
        held.session_state(),
        ConnectionState::Direct,
        "supersede must not flip the connection state"
    );
    assert!(
        !held.inner.unmounted.load(Ordering::Relaxed),
        "supersede is not an unmount"
    );

    // The watcher IS retired, though: the successor spawns its own on its own
    // session, and two watchers on one volume id double-feed the index and let
    // the retired one's death path reach the successor.
    assert_eq!(
        held.listing_watch_coverage(Path::new("/")),
        WatchCoverage::None,
        "the superseded volume's watcher must be cancelled"
    );

    // And a genuine unmount still tears everything down.
    let for_unmount = Arc::clone(&vol);
    tokio::task::spawn_blocking(move || for_unmount.on_unmount())
        .await
        .expect("on_unmount runs on a blocking thread");
    assert!(
        matches!(
            held.list_directory_impl(Path::new("")).await,
            Err(VolumeError::DeviceDisconnected(_))
        ),
        "an actual unmount still drops the session"
    );
}

/// A retired volume isn't a dead end for the holders still on it: if its
/// connection breaks mid-transfer, it rebuilds in place, because the running
/// copy holds THIS instance and can't switch to the successor. What it must not
/// do is respawn the watcher, which belongs to the volume id the successor now
/// owns (two watchers double-feed the index, and the retired one's death path
/// reaches the successor).
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_superseded_volume_reconnects_without_reclaiming_the_id() {
    let vol = make_docker_volume().await;
    vol.on_superseded();
    assert!(
        vol.inner.watcher_cancel.lock().unwrap().is_none(),
        "supersede cancelled the watcher"
    );

    // "The server hung up mid-copy": drop the session under the holder.
    {
        let mut client_guard = vol.inner.client.lock().await;
        *client_guard = None;
    }
    {
        let mut tree_guard = vol.inner.tree.write().await;
        *tree_guard = None;
    }
    vol.inner.transition_to_disconnected();

    vol.inner
        .do_attempt_reconnect()
        .await
        .expect("a retired volume still rebuilds for the operation holding it");
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert!(
        vol.list_directory_impl(Path::new("")).await.is_ok(),
        "the holder's work continues on the rebuilt session"
    );
    assert!(
        vol.inner.watcher_cancel.lock().unwrap().is_none(),
        "no watcher may be respawned for an id this volume no longer owns"
    );
}

// ── Socket lifetime (mount/unmount cycles) ───────────────────────

/// The remote ends of this process's TCP sockets connected to `port` on any
/// address, read with `getpeername` over every open fd.
///
/// In-process rather than `lsof`, so the cell runs wherever the lane does. A
/// socket the server hung up on (`CLOSE_WAIT`) still has a peer, so this sees
/// both shapes `smb2`'s socket-lifetime bugs left behind.
#[cfg(unix)]
fn tcp_peers_on_port(port: u16) -> Vec<std::net::SocketAddr> {
    // SAFETY: `getdtablesize` takes no arguments and only reads the process's
    // descriptor-table limit.
    let max_fd = unsafe { libc::getdtablesize() };
    (0..max_fd)
        .filter_map(|fd| {
            // SAFETY: `sockaddr_storage` is plain old data, for which all-zero
            // bytes are a valid (`AF_UNSPEC`) value.
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut len = size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            // SAFETY: `storage` is a correctly-sized, writable `sockaddr_storage`
            // and `len` holds its size, as `getpeername` requires; an fd that
            // isn't an open, connected socket makes it return -1 and write nothing.
            let rc = unsafe { libc::getpeername(fd, (&raw mut storage).cast::<libc::sockaddr>(), &mut len) };
            if rc != 0 {
                return None;
            }
            peer_addr(&storage)
        })
        .filter(|peer| peer.port() == port)
        .collect()
}

#[cfg(unix)]
fn peer_addr(storage: &libc::sockaddr_storage) -> Option<std::net::SocketAddr> {
    match i32::from(storage.ss_family) {
        libc::AF_INET => {
            // SAFETY: the family says the kernel wrote a `sockaddr_in`, which
            // fits inside `sockaddr_storage` and shares its alignment.
            let sin = unsafe { &*std::ptr::from_ref(storage).cast::<libc::sockaddr_in>() };
            let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            Some(std::net::SocketAddr::from((ip, u16::from_be(sin.sin_port))))
        }
        libc::AF_INET6 => {
            // SAFETY: the family says the kernel wrote a `sockaddr_in6`, which
            // fits inside `sockaddr_storage` and shares its alignment.
            let sin6 = unsafe { &*std::ptr::from_ref(storage).cast::<libc::sockaddr_in6>() };
            let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
            Some(std::net::SocketAddr::from((ip, u16::from_be(sin6.sin6_port))))
        }
        _ => None,
    }
}

/// Mount/unmount cycles leave no SMB sockets behind: every session a mount opens
/// (the main one, the watcher's, the scan pool's) closes once the volume is
/// unmounted and dropped.
///
/// Pins the two `smb2` socket-lifetime fixes (0.24.1) in Cmdr's own usage:
/// before them, 60 E2E mount/unmount cycles left 78 ESTABLISHED sockets in one
/// process, because a dropped `Connection` never closed its socket while the
/// server kept it open (the fixtures never reap idle sessions). See
/// `docs/notes/performance/smb2-socket-lifetime-2026-09-23.md`.
///
/// Counts every socket to the fixture's port, so it relies on nextest's
/// process-per-test: under `cargo test`, a sibling cell's live session would
/// count too.
#[cfg(unix)]
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_mount_unmount_cycles_leave_no_sockets_behind() {
    const CYCLES: usize = 10;
    let port = guest_port();
    assert_eq!(
        tcp_peers_on_port(port),
        Vec::new(),
        "this process holds no fixture sockets before the first mount"
    );

    for _ in 0..CYCLES {
        let vol = make_docker_volume().await;
        vol.list_directory_impl(Path::new(""))
            .await
            .expect("the fresh mount lists its root");
        vol.begin_scan_session().await;
        assert!(
            vol.inner.scan_pool.read().await.is_some(),
            "the scan pool's sessions are up, so the cycle covers them too"
        );
        assert!(
            tcp_peers_on_port(port).len() > 1,
            "a scanning mount holds more than one session"
        );
        // `on_unmount` is sync and takes its locks blocking, the way the FSEvents
        // thread calls it, so it runs off the runtime.
        tokio::task::spawn_blocking(move || {
            vol.on_unmount();
            drop(vol);
        })
        .await
        .expect("unmount runs to completion");
    }

    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(15),
        "every fixture socket to close after the last unmount",
        || tcp_peers_on_port(port).is_empty(),
    )
    .await;
}
