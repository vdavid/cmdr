//! The saved servers both server suites check against (`propose_server`'s and the approval's):
//! an SFTP build box, a WebDAV account, an NFS export, an S3 account with one bucket, and an SMB
//! host (Testnas) with two saved shares.

use super::saved::{SavedPlaceDraft, SavedRow, SavedServers};
use crate::agent::store::proposals::ServerDraft;
use crate::agent::types::DraftProtocol;
use crate::network::s3_known_places::S3ProviderChoice;

pub(crate) const SFTP_ID: &str = "sftp-build-box";
pub(crate) const WEBDAV_ID: &str = "webdav-photos";
pub(crate) const NFS_ID: &str = "nfs-public";
pub(crate) const S3_ACCOUNT: &str = "s3-account";
pub(crate) const S3_BUCKET_ID: &str = "s3-invoices";
pub(crate) const SMB_SHARE: &str = "smb-testnas-media";
/// The SMB host's row id.
pub(crate) const SMB_HOST: &str = "testnas.local";

pub(crate) fn row(id: &str, name: &str, protocol: DraftProtocol, places: Vec<(&str, ServerDraft)>) -> SavedRow {
    SavedRow {
        id: id.to_string(),
        name: name.to_string(),
        protocol,
        places: places
            .into_iter()
            .map(|(volume_id, draft)| SavedPlaceDraft {
                volume_id: volume_id.to_string(),
                draft,
            })
            .collect(),
        smb_host: None,
        s3_account: None,
    }
}

pub(crate) fn sftp_saved() -> ServerDraft {
    ServerDraft {
        display_name: Some("Build box".into()),
        host: Some("build.lan".into()),
        port: Some(22),
        username: Some("test_user_13".into()),
        remote_root: Some("/srv".into()),
        ..ServerDraft::new(DraftProtocol::Sftp)
    }
}

pub(crate) fn smb_saved() -> ServerDraft {
    ServerDraft {
        display_name: Some("Testnas".into()),
        host: Some("testnas.local".into()),
        port: Some(445),
        username: Some("test_user_3".into()),
        shares_affected: Some(2),
        ..ServerDraft::new(DraftProtocol::Smb)
    }
}

pub(crate) fn saved() -> SavedServers {
    // Real ids from the id funnel, so an add of the same address meets the saved one.
    let sftp_id = cmdr_fs::volume::sftp_volume_id("build.lan", 22, "test_user_13");
    let webdav_id = cmdr_fs::volume::webdav_volume_id("dav.example.com", 443, "test_user_1");
    let nfs_id = cmdr_fs::volume::nfs_volume_id("files.lan", 2049, "/Public");
    let s3 = crate::network::s3_known_places::KnownS3Place {
        provider: S3ProviderChoice::Aws {
            region: "eu-west-1".into(),
        },
        access_key_id: "AKIAEXAMPLE".into(),
        bucket: Some("invoices".into()),
        auto_reconnect: true,
        pinned: true,
        last_connected_at: String::new(),
    };
    let s3_bucket_id = s3.volume_id().expect("an AWS place has an id");
    let mut s3_row = row(
        S3_ACCOUNT,
        "AKIAEXAMPLE@s3",
        DraftProtocol::S3,
        vec![(
            &s3_bucket_id,
            ServerDraft {
                account_id: Some(S3_ACCOUNT.into()),
                bucket: Some("invoices".into()),
                ..ServerDraft::new(DraftProtocol::S3)
            },
        )],
    );
    s3_row.s3_account = Some((s3.provider.clone(), s3.access_key_id.clone()));
    let mut smb = row(
        SMB_HOST,
        "Testnas",
        DraftProtocol::Smb,
        vec![(SMB_SHARE, smb_saved()), ("smb-testnas-backup", smb_saved())],
    );
    smb.smb_host = Some(("testnas.local".into(), 445));
    let rows = vec![
        row(
            &sftp_id,
            "Build box",
            DraftProtocol::Sftp,
            vec![(&sftp_id, sftp_saved())],
        ),
        row(
            &webdav_id,
            "Photos",
            DraftProtocol::Webdav,
            vec![(
                &webdav_id,
                ServerDraft {
                    display_name: Some("Photos".into()),
                    url: Some("https://dav.example.com/dav/".into()),
                    username: Some("test_user_1".into()),
                    remote_root: Some("/Photos".into()),
                    ..ServerDraft::new(DraftProtocol::Webdav)
                },
            )],
        ),
        row(
            "nfs-files",
            "files.lan",
            DraftProtocol::Nfs,
            vec![(
                &nfs_id,
                ServerDraft {
                    host: Some("files.lan".into()),
                    export: Some("/Public".into()),
                    uid: Some(1000),
                    gid: Some(1000),
                    ..ServerDraft::new(DraftProtocol::Nfs)
                },
            )],
        ),
        s3_row,
        smb,
    ];
    // The aliases the tests name places by.
    let mut saved = SavedServers { rows };
    for (alias, real) in [
        (SFTP_ID, sftp_id),
        (WEBDAV_ID, webdav_id),
        (NFS_ID, nfs_id),
        (S3_BUCKET_ID, s3_bucket_id),
    ] {
        let place = saved
            .rows
            .iter()
            .flat_map(|row| row.places.iter())
            .find(|place| place.volume_id == real)
            .cloned()
            .expect("aliased place");
        let row = saved
            .rows
            .iter_mut()
            .find(|row| row.places.iter().any(|p| p.volume_id == real))
            .expect("its row");
        row.places.push(SavedPlaceDraft {
            volume_id: alias.to_string(),
            draft: place.draft,
        });
    }
    saved
}
