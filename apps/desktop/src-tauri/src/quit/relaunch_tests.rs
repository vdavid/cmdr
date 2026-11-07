use super::*;
use crate::test_support::{TestDir, wait_until};
use cmdr_fs::testing::holds_within;
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command};

fn plan(pid: u32, opener: PathBuf, args: &[&str]) -> WaiterPlan {
    WaiterPlan {
        pid,
        ticks: 300,
        interval: "0.05".to_string(),
        opener,
        bundle: PathBuf::from("/Applications/Cmdr.app"),
        args: args.iter().map(OsString::from).collect(),
    }
}

#[test]
fn the_waiter_runs_sh_with_the_bundle_and_our_args_as_positional_params() {
    let (program, argv) = waiter_command(&plan(4242, PathBuf::from("/usr/bin/open"), &["--flag", "a b"]));
    assert_eq!(program, PathBuf::from("/bin/sh"));
    assert_eq!(argv[0], "-c");
    let expected: Vec<OsString> = [
        "cmdr-relaunch",
        "4242",
        "300",
        "0.05",
        "/usr/bin/open",
        "/Applications/Cmdr.app",
        "--flag",
        "a b",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    assert_eq!(argv[2..], expected[..]);
}

/// The whole bug (#1037): `open -n` starts a second instance, and the Dock gives it its own tile.
#[test]
fn the_waiter_never_opens_a_new_instance() {
    let (_, argv) = waiter_command(&plan(1, PathBuf::from("/usr/bin/open"), &[]));
    let script = argv[1].to_string_lossy();
    assert!(
        !script.contains("-n"),
        "the waiter's script must not pass -n to open:\n{script}"
    );
    assert!(script.contains(r#""$opener" "$bundle" --args "$@""#));
}

#[test]
fn bundle_root_finds_the_app_around_its_binary() {
    assert_eq!(
        bundle_root(Path::new("/Applications/Cmdr.app/Contents/MacOS/Cmdr")),
        Some(PathBuf::from("/Applications/Cmdr.app"))
    );
    assert_eq!(bundle_root(Path::new("/repo/target/debug/Cmdr")), None);
    assert_eq!(bundle_root(Path::new("/x/Contents/MacOS/Cmdr")), None);
    assert_eq!(bundle_root(Path::new("/x/Cmdr.app/MacOS/Cmdr")), None);
}

/// A stand-in for `/usr/bin/open` that records the args it got into `marker`.
fn recording_opener(dir: &TestDir) -> (PathBuf, PathBuf) {
    let marker = dir.join("opened");
    let opener = dir.join("opener.sh");
    std::fs::write(
        &opener,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", marker.display()),
    )
    .expect("write the stand-in opener");
    std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).expect("make the opener executable");
    (opener, marker)
}

fn long_sleeper() -> Child {
    Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .expect("spawn a process to outlive")
}

#[test]
fn the_waiter_opens_only_after_the_process_exits() {
    let dir = TestDir::new("relaunch_waits");
    let (opener, marker) = recording_opener(&dir);
    let mut old = long_sleeper();

    spawn_waiter(&plan(old.id(), opener, &["--x"])).expect("start the waiter");
    assert!(
        !holds_within(Duration::from_millis(500), || marker.exists()),
        "the waiter opened while the old process was still alive"
    );

    old.kill().expect("end the old process");
    // Reaped, or `kill -0` keeps seeing the zombie.
    old.wait().expect("reap the old process");
    wait_until(Duration::from_secs(5), "the waiter opens the bundle", || {
        marker.exists()
    });
    wait_until(Duration::from_secs(5), "the opener wrote its args", || {
        std::fs::read_to_string(&marker).is_ok_and(|s| s == "/Applications/Cmdr.app\n--args\n--x\n")
    });
}

#[test]
fn the_waiter_opens_anyway_once_its_wait_runs_out() {
    let dir = TestDir::new("relaunch_bounded");
    let (opener, marker) = recording_opener(&dir);
    let mut stuck = long_sleeper();

    let mut bounded = plan(stuck.id(), opener, &[]);
    bounded.ticks = 4;
    spawn_waiter(&bounded).expect("start the waiter");
    let opened = holds_within(Duration::from_secs(5), || marker.exists());

    stuck.kill().expect("end the stuck process");
    stuck.wait().expect("reap the stuck process");
    assert!(opened, "the waiter never gave up waiting on a process that didn't exit");
}
