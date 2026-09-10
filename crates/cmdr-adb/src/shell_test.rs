use super::*;
use crate::testing::pixel_captures::{
    DF_K_ALL, DF_K_FOUND_AND_MISSING, DF_K_MISSING, DF_K_ROOT, DF_K_SHARED_STORAGE, DF_K_THREE_PATHS, data_row,
};
use crate::testing::{FAKE_SERIAL, FakeAdbServer, FakeMount, FakeTree, split_argv};

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
fn parse_df_k_reads_a_pixels_real_output() {
    assert_eq!(
        parse_df_k(DF_K_SHARED_STORAGE),
        Some(SpaceParts {
            total_bytes: 114_786_388 * 1024,
            available_bytes: 26_956_476 * 1024,
        })
    );
    // The read-only system image really does report nothing free, which is
    // why the volume never asks about `/` for the phone's figure.
    assert_eq!(
        parse_df_k(DF_K_ROOT),
        Some(SpaceParts {
            total_bytes: 904_496 * 1024,
            available_bytes: 0,
        })
    );
    // A missing path still prints the header: no figures, never a zero.
    assert_eq!(parse_df_k(DF_K_MISSING), None);
    // Beside a missing path the found row still parses; only the exit code
    // says something failed.
    assert_eq!(
        parse_df_k(DF_K_FOUND_AND_MISSING).map(|p| p.available_bytes),
        Some(26_951_288 * 1024)
    );
}

#[test]
fn parse_df_k_reads_busybox_layouts() {
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
    assert_eq!(space.total_bytes, 114_786_388 * 1024);
}

/// ❗ The fake's `df` answers byte for byte what a Pixel printed
/// (`testing::pixel_captures`): the mount point in the last column, toybox's
/// per-invocation column widths, and the read-only system image at `/`.
#[tokio::test(flavor = "multi_thread")]
async fn the_fakes_df_prints_what_a_pixel_prints() {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard/Download").add_dir("/sdcard/DCIM/Camera");
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();
    for path in ["/sdcard", "/sdcard/", "/sdcard/Download", "/sdcard/DCIM/Camera"] {
        let out = run(&ep, FAKE_SERIAL, &["df", "-k", path]).await.unwrap();
        assert_eq!(out.stdout_text(), DF_K_SHARED_STORAGE, "df -k {path}");
        assert!(out.succeeded(), "df -k {path}: {out:?}");
    }
    let out = run(&ep, FAKE_SERIAL, &["df", "-k", "/"]).await.unwrap();
    assert_eq!(out.stdout_text(), DF_K_ROOT);
    assert!(out.succeeded(), "{out:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fakes_df_on_a_missing_path_prints_the_header_and_exits_1() {
    let mut tree = FakeTree::new();
    tree.mount(FakeMount::with_row("/sdcard", data_row(DF_K_FOUND_AND_MISSING, 0)));
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();

    let out = run(&ep, FAKE_SERIAL, &["df", "-k", "/nonexistent/path"]).await.unwrap();
    assert_eq!(out.stdout_text(), DF_K_MISSING);
    assert_eq!(out.exit_code, 1);
    assert!(!out.stderr.is_empty(), "the reason goes to stderr, for people");

    let out = run(&ep, FAKE_SERIAL, &["df", "-k", "/sdcard", "/nonexistent/path"])
        .await
        .unwrap();
    assert_eq!(out.stdout_text(), DF_K_FOUND_AND_MISSING);
    assert_eq!(out.exit_code, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fakes_df_sizes_one_table_over_every_path_asked() {
    let mut tree = FakeTree::new();
    tree.add_dir("/storage/emulated/0")
        .add_symlink("/storage/self/primary", "/storage/emulated/0")
        .add_dir("/data/local/tmp")
        .mount(FakeMount::with_row("/storage/emulated", data_row(DF_K_THREE_PATHS, 0)))
        // A bind mount: `/data` reports `/data/user/0` as its mount point.
        .mount(FakeMount::with_row("/data", data_row(DF_K_THREE_PATHS, 2)));
    let server = FakeAdbServer::start(tree).await;
    let ep = server.endpoint();
    let argv = [
        "df",
        "-k",
        "/storage/emulated",
        "/storage/self/primary",
        "/data/local/tmp",
    ];
    let out = run(&ep, FAKE_SERIAL, &argv).await.unwrap();
    assert_eq!(out.stdout_text(), DF_K_THREE_PATHS);
    assert!(out.succeeded(), "{out:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fakes_bare_df_lays_out_every_mount_as_toybox_does() {
    let mut tree = FakeTree::new();
    tree.unmount_all();
    for row in DF_K_ALL.lines().skip(1) {
        let mount_point = row.split_whitespace().last().expect("a row ends in its mount point");
        tree.mount(FakeMount::with_row(mount_point, row));
    }
    let server = FakeAdbServer::start(tree).await;
    let out = run(&server.endpoint(), FAKE_SERIAL, &["df", "-k"]).await.unwrap();
    assert_eq!(out.stdout_text(), DF_K_ALL);
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
