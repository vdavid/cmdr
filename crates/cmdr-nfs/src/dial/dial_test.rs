use super::*;
use crate::proto::rpc::AuthStat;
use crate::xdr::XdrError;

#[test]
fn the_longest_listed_export_wins_the_split() {
    let exports = vec!["/share".to_string(), "/share/Public".to_string(), "/other".to_string()];
    assert_eq!(longest_export(&exports, "/share/Public/photos"), Some("/share/Public"));
    assert_eq!(longest_export(&exports, "/share/Public"), Some("/share/Public"));
    assert_eq!(longest_export(&exports, "/share/Private"), Some("/share"));
    // A shared prefix of characters isn't a shared folder.
    assert_eq!(longest_export(&exports, "/shared"), None);
    assert_eq!(longest_export(&["/".to_string()], "/anything"), Some("/"));
}

#[test]
fn the_mnt_walk_tries_the_whole_path_first_and_ends_at_the_root() {
    assert_eq!(prefixes("/a/b/c"), vec!["/a/b/c", "/a/b", "/a", "/"]);
    assert_eq!(prefixes("/a"), vec!["/a", "/"]);
    assert_eq!(prefixes("/"), vec!["/"]);
}

#[test]
fn the_rest_of_the_typed_path_is_the_start_folder() {
    assert_eq!(
        folder_under("/share", "/share/photos/2024"),
        Some("/photos/2024".to_string())
    );
    assert_eq!(folder_under("/share", "/share"), None);
    assert_eq!(folder_under("/", "/photos"), Some("/photos".to_string()));
    assert_eq!(folder_under("/", "/"), None);
}

#[test]
fn an_export_naming_no_flavors_takes_auth_sys_and_a_kerberos_only_one_does_not() {
    assert!(accepts_auth_sys(&[]));
    assert!(accepts_auth_sys(&[390_003, 1]));
    assert!(!accepts_auth_sys(&[390_003, 390_004]));
}

#[test]
fn a_refusal_names_the_port_only_when_the_dial_had_no_reserved_one() {
    // knfsd's answers to a non-reserved port, measured against the fixture.
    let mnt_access = refusal(ReservedPort::Unavailable);
    assert!(matches!(mnt_access, NfsConnectError::ReservedPortRequired));
    let nfs_perm = nfs_failure(&NfsError::Status(NfsStat3::Perm), ReservedPort::Unavailable);
    assert!(matches!(nfs_perm, NfsConnectError::ReservedPortRequired));
    let too_weak = call_failure(
        &CallError::Denied(Rejection::Auth(AuthStat::TooWeak)),
        ReservedPort::Unavailable,
    );
    assert!(matches!(too_weak, NfsConnectError::ReservedPortRequired));
    // The same answers from a reserved port: the export refuses this client.
    let refused = nfs_failure(&NfsError::Status(NfsStat3::Access), ReservedPort::Bound(900));
    assert!(matches!(refused, NfsConnectError::ExportRefused));
    let too_weak = call_failure(
        &CallError::Denied(Rejection::Auth(AuthStat::TooWeak)),
        ReservedPort::Bound(900),
    );
    assert!(matches!(too_weak, NfsConnectError::ExportRefused));
}

#[test]
fn a_server_without_v3_is_no_nfs_v3_and_never_a_fallback() {
    let mismatch = call_failure(
        &CallError::NotAccepted(AcceptError::ProgramMismatch { low: 4, high: 4 }),
        ReservedPort::Bound(900),
    );
    assert!(matches!(mismatch, NfsConnectError::NoNfsV3));
    let missing = call_failure(
        &CallError::NotAccepted(AcceptError::ProgramUnavailable),
        ReservedPort::Bound(900),
    );
    assert!(matches!(missing, NfsConnectError::NoNfsV3));
}

#[test]
fn transport_failures_during_the_dial_map_to_reach_and_time() {
    assert!(matches!(
        call_failure(&CallError::Disconnected("reset".into()), ReservedPort::NotAsked),
        NfsConnectError::Unreachable(_)
    ));
    assert!(matches!(
        call_failure(&CallError::TimedOut, ReservedPort::NotAsked),
        NfsConnectError::TimedOut
    ));
    assert!(matches!(
        call_failure(
            &CallError::Decode(XdrError::Truncated { needed: 4, left: 0 }),
            ReservedPort::NotAsked
        ),
        NfsConnectError::Transport(_)
    ));
    assert!(matches!(
        nfs_failure(&NfsError::Status(NfsStat3::Stale), ReservedPort::Bound(900)),
        NfsConnectError::ExportNotFound
    ));
}

#[tokio::test]
async fn nothing_listening_is_unreachable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let server = NfsServer {
        host: "127.0.0.1".to_string(),
        port: None,
        portmap_port: Some(port),
    };
    let err = list_exports(&server, &DialOptions::PRODUCTION, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, NfsConnectError::Unreachable(_)), "{err:?}");
}

#[tokio::test]
async fn a_cancelled_dial_answers_cancelled() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = mount(
        &NfsServer::new("127.0.0.1", None),
        "/x",
        &NfsIdentity::new(1, 1),
        &DialOptions::PRODUCTION,
        &cancel,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(err, NfsConnectError::Cancelled), "{err:?}");
}

#[test]
fn a_credential_carries_the_other_groups_on_the_wire() {
    let Credential::Sys(body) = credential_for(&NfsIdentity {
        uid: 1000,
        gid: 100,
        other_gids: vec![2001, 2002],
    }) else {
        panic!("expected AUTH_SYS");
    };
    let mut dec = crate::xdr::Decoder::new(body);
    dec.u32().unwrap(); // stamp
    dec.opaque(255).unwrap(); // machine name
    assert_eq!((dec.u32().unwrap(), dec.u32().unwrap()), (1000, 100));
    assert_eq!(dec.u32().unwrap(), 3);
    let gids: Vec<u32> = (0..3).map(|_| dec.u32().unwrap()).collect();
    assert_eq!(gids, vec![100, 2001, 2002]);
}
