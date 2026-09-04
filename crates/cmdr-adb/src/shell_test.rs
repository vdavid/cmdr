use super::*;
use crate::testing::{FAKE_SERIAL, FakeAdbServer, FakeTree, split_argv};

#[test]
fn quoting_is_posix_single_quotes() {
    assert_eq!(quote("plain"), "'plain'");
    assert_eq!(quote(""), "''");
    assert_eq!(quote("it's"), "'it'\\''s'");
    assert_eq!(quote("a b$c\"d"), "'a b$c\"d'");
    assert_eq!(
        command_line(&["mv", "/sdcard/a's", "/sdcard/b"]),
        "'mv' '/sdcard/a'\\''s' '/sdcard/b'"
    );
}

#[test]
fn the_fake_shell_unquotes_what_command_line_quotes() {
    let argv = ["mv", "/sdcard/it's here", "/x y", "", "tab\there"];
    assert_eq!(
        split_argv(&command_line(&argv)),
        argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn parse_df_k_reads_toybox_and_busybox() {
    let toybox = "Filesystem      1K-blocks     Used Available Use% Mounted on\n/dev/fuse       118120468 21356460  96764008  19% /storage/emulated\n";
    assert_eq!(
        parse_df_k(toybox),
        Some(SpaceParts {
            total_bytes: 118_120_468 * 1024,
            available_bytes: 96_764_008 * 1024,
        })
    );
    let busybox = "Filesystem           1K-blocks      Used Available Use% Mounted on\n/dev/block/dm-0      118120468  21356460  96764008  18% /data\n";
    assert_eq!(parse_df_k(busybox).unwrap().total_bytes, 118_120_468 * 1024);
    let wrapped = "Filesystem           1K-blocks      Used Available Use% Mounted on\n/dev/block/platform/soc/1d84000.ufshc/by-name/userdata\n                     118120468  21356460  96764008  18% /data\n";
    assert_eq!(parse_df_k(wrapped).unwrap().available_bytes, 96_764_008 * 1024);
    assert_eq!(parse_df_k(""), None);
    assert_eq!(parse_df_k("Filesystem 1K-blocks\n"), None);
    assert_eq!(parse_df_k("df: /nope: No such file or directory\n"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_mutations_against_the_fake_tree() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/a.txt", b"a");
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();

    let out = run(&ep, FAKE_SERIAL, &["mkdir", "-p", "/sdcard/new/deep"])
        .await
        .unwrap();
    assert!(out.succeeded(), "{out:?}");
    let out = run(&ep, FAKE_SERIAL, &["mv", "/sdcard/a.txt", "/sdcard/new/deep/b.txt"])
        .await
        .unwrap();
    assert!(out.succeeded(), "{out:?}");
    assert!(
        run(&ep, FAKE_SERIAL, &["test", "-e", "/sdcard/new/deep/b.txt"])
            .await
            .unwrap()
            .succeeded()
    );
    assert!(
        !run(&ep, FAKE_SERIAL, &["test", "-e", "/sdcard/a.txt"])
            .await
            .unwrap()
            .succeeded()
    );

    let out = run(&ep, FAKE_SERIAL, &["rm", "-rf", "/sdcard/new"]).await.unwrap();
    assert!(out.succeeded());
    assert!(server.tree().lock().unwrap().get("/sdcard/new").is_none());

    let out = run(&ep, FAKE_SERIAL, &["mv", "/sdcard/ghost", "/sdcard/x"])
        .await
        .unwrap();
    assert_eq!(out.exit_code, 1);
    assert!(!out.stderr.is_empty());

    let out = run(&ep, FAKE_SERIAL, &["frobnicate"]).await.unwrap();
    assert_eq!(out.exit_code, 127);

    let out = run(&ep, FAKE_SERIAL, &["df", "-k", "/sdcard"]).await.unwrap();
    assert!(out.succeeded());
    let space = parse_df_k(&out.stdout_text()).unwrap();
    assert_eq!(space.total_bytes, 118_120_468 * 1024);
}

#[tokio::test(flavor = "multi_thread")]
async fn readlink_and_stat_probe() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/real.txt", b"12345")
        .add_symlink("/sdcard/link", "real.txt");
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();
    let out = run(&ep, FAKE_SERIAL, &["readlink", "-f", "/sdcard/link"])
        .await
        .unwrap();
    assert_eq!(out.stdout_text().trim(), "/sdcard/real.txt");
    let out = run(&ep, FAKE_SERIAL, &["stat", "-c", "%f %s %Y", "/sdcard/real.txt"])
        .await
        .unwrap();
    assert_eq!(
        out.stdout_text().trim(),
        format!("81a4 5 {}", crate::testing::DEFAULT_MTIME)
    );
    let out = run(&ep, FAKE_SERIAL, &["stat", "-c", "%f %s %Y", "/sdcard/nope"])
        .await
        .unwrap();
    assert_eq!(out.exit_code, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn readlink_of_an_absolute_link_to_a_folder() {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard")
        .add_dir("/sdcard/DCIM")
        .add_symlink("/sdcard/shortcut", "/sdcard/DCIM");
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();
    let out = run(&ep, FAKE_SERIAL, &["readlink", "-f", "/sdcard/shortcut"])
        .await
        .unwrap();
    assert!(out.succeeded(), "{out:?}");
    assert_eq!(out.stdout_text().trim(), "/sdcard/DCIM");
    let mut session = crate::sync::SyncSession::open(&ep, FAKE_SERIAL, crate::features::DeviceFeatures::all())
        .await
        .unwrap();
    let stat = session.stat(out.stdout_text().trim()).await.unwrap();
    assert_eq!(stat.kind(), crate::sync::SyncEntryKind::Directory, "{stat:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rmdir_cp_and_test_flags() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/full/x.txt", b"x").add_dir("/sdcard/empty");
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();
    assert_eq!(
        run(&ep, FAKE_SERIAL, &["rmdir", "/sdcard/full"])
            .await
            .unwrap()
            .exit_code,
        1
    );
    assert!(
        run(&ep, FAKE_SERIAL, &["rmdir", "/sdcard/empty"])
            .await
            .unwrap()
            .succeeded()
    );
    assert!(
        run(&ep, FAKE_SERIAL, &["cp", "-f", "/sdcard/full/x.txt", "/sdcard/y.txt"])
            .await
            .unwrap()
            .succeeded()
    );
    assert_eq!(server.tree().lock().unwrap().file_bytes("/sdcard/y.txt").unwrap(), b"x");
    assert!(
        run(&ep, FAKE_SERIAL, &["test", "-w", "/sdcard"])
            .await
            .unwrap()
            .succeeded()
    );
    assert!(
        run(&ep, FAKE_SERIAL, &["test", "-d", "/sdcard"])
            .await
            .unwrap()
            .succeeded()
    );
    assert!(
        !run(&ep, FAKE_SERIAL, &["test", "-f", "/sdcard"])
            .await
            .unwrap()
            .succeeded()
    );
    server.tree().lock().unwrap().read_only = true;
    assert!(
        !run(&ep, FAKE_SERIAL, &["test", "-w", "/sdcard"])
            .await
            .unwrap()
            .succeeded()
    );
}
