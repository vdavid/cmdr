use super::*;

#[test]
fn the_e2e_portmapper_override_reads_a_port_and_ignores_anything_else() {
    assert_eq!(portmap_port_override(Some("15480")), Some(15480));
    assert_eq!(portmap_port_override(Some(" 15480\n")), Some(15480));
    // Unset, empty, zero, or not a port: the standard 111.
    assert_eq!(portmap_port_override(None), None);
    assert_eq!(portmap_port_override(Some("")), None);
    assert_eq!(portmap_port_override(Some("0")), None);
    assert_eq!(portmap_port_override(Some("111111")), None);
    assert_eq!(portmap_port_override(Some("nas:111")), None);
}

#[cfg(not(feature = "playwright-e2e"))]
#[test]
fn a_build_without_the_e2e_feature_always_asks_portmap_on_111() {
    // The override is compiled out: nothing in the environment can move it.
    assert_eq!(NfsServer::new("nas", None).portmap_port, None);
}

#[test]
fn a_place_with_no_port_is_minted_on_the_nfs_port() {
    // Identity can't wait for portmap: a saved place must name one id before
    // anything is dialed, and the same one after.
    let params = NfsConnectionParams::new(
        NfsServer::new("TESTNAS.local", None),
        "/Public/",
        NfsIdentity::new(1000, 100),
    );
    assert_eq!(params.export(), "/Public");
    assert_eq!(
        params.volume_id(),
        cmdr_fs::volume::nfs_volume_id("testnas.local", NFS_PORT, "/Public")
    );
    assert_eq!(params.app_root(), "nfs://testnas.local:2049/Public");
}

#[test]
fn the_identity_never_reaches_the_volume_id() {
    let as_ada = NfsConnectionParams::new(NfsServer::new("nas", Some(2049)), "/x", NfsIdentity::new(1000, 100));
    let as_bob = NfsConnectionParams::new(NfsServer::new("nas", Some(2049)), "/x", NfsIdentity::new(1001, 100));
    assert_eq!(as_ada.volume_id(), as_bob.volume_id());
}

fn with_others(gid: u32, other_gids: Vec<u32>) -> NfsIdentity {
    NfsIdentity {
        uid: 1000,
        gid,
        other_gids,
    }
}

#[test]
fn someone_in_their_primary_group_alone_sends_just_that_group() {
    // Regression anchor: what every place saved before `other_gids` keeps sending.
    assert_eq!(NfsIdentity::new(1000, 100).credential_gids(), vec![100]);
}

#[test]
fn the_other_groups_follow_the_primary_each_once() {
    let identity = with_others(100, vec![2001, 100, 2002, 2001]);
    assert_eq!(identity.credential_gids(), vec![100, 2001, 2002]);
}

#[test]
fn sixteen_others_all_go_out_and_the_primary_rides_the_gid_field_alone() {
    let others: Vec<u32> = (2000..2016).collect();
    assert_eq!(with_others(100, others.clone()).credential_gids(), others);
}

#[test]
fn fifteen_others_still_leave_room_for_the_primary() {
    let others: Vec<u32> = (2000..2015).collect();
    let gids = with_others(100, others.clone()).credential_gids();
    assert_eq!(gids.len(), 16);
    assert_eq!(gids[0], 100);
    assert_eq!(&gids[1..], others.as_slice());
}

#[test]
fn a_list_past_the_rfc_cap_keeps_its_first_sixteen() {
    // The form refuses this; a hand-edited store file can still say it.
    let gids = with_others(100, (2000..2040).collect()).credential_gids();
    assert_eq!(gids, (2000..2016).collect::<Vec<u32>>());
}

#[test]
fn the_other_groups_stay_out_of_the_volume_id_and_the_app_root() {
    let plain = NfsConnectionParams::new(NfsServer::new("nas", None), "/Public", NfsIdentity::new(1000, 100));
    let grouped = NfsConnectionParams::new(NfsServer::new("nas", None), "/Public", with_others(100, vec![2001]));
    assert_eq!(plain.volume_id(), grouped.volume_id());
    assert_eq!(plain.app_root(), grouped.app_root());
}
