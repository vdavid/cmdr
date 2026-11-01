//! The `cmdr` script (`resources/bin/cmdr`) and the app's parser, tested as the pair they are.
//!
//! Each test runs the real script with `--print-url`, then feeds what it printed to
//! [`parse_cli_url`]. So the encoding the script writes and the decoding the app does can't drift
//! apart: a change to either side that breaks the other fails here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tauri::Url;

use super::open_url::{CliOpen, parse_cli_url};
use crate::test_support::TestDir;
use crate::tools::{GoToTarget, PaneChoice};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/bin/cmdr")
}

fn run_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(script())
        .args(args)
        .current_dir(cwd)
        .env_remove("CMDR_BUNDLE_ID")
        .output()
        .expect("the script runs")
}

/// Run with `--print-url` and parse what comes out, failing loudly if either side refuses.
fn open_in(cwd: &Path, args: &[&str]) -> (CliOpen, String) {
    let mut all = vec!["--print-url"];
    all.extend_from_slice(args);
    let output = run_in(cwd, &all);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "the script refused: {stderr}");
    let url = String::from_utf8(output.stdout).expect("a UTF-8 URL");
    let open = parse_cli_url(&Url::parse(url.trim()).expect("a URL")).expect("the app accepts it");
    (open, stderr)
}

/// The scratch dir as the script's `pwd` reports it. A real terminal hands the script a `PWD`
/// spelled the way the user got there (symlinks kept); a test sets the cwd with `chdir`, so the
/// shell starts from the physical path (`/private/var/…` for macOS's temp dir).
fn logical(dir: &TestDir) -> PathBuf {
    dir.canonicalize().expect("the scratch dir resolves")
}

#[test]
fn a_file_with_every_awkward_character_round_trips() {
    let dir = TestDir::new("cli_script_awkward");
    let name = "a b+c&d=e#f%g'h\"é😀.txt";
    std::fs::write(dir.join(name), b"x").expect("write");
    let (open, stderr) = open_in(&dir, &[name]);
    assert_eq!(open.paths, vec![logical(&dir).join(name)]);
    assert_eq!(open.target, GoToTarget::FOCUSED);
    assert!(stderr.is_empty(), "no warning for one path: {stderr}");
}

#[test]
fn no_path_means_the_current_folder() {
    let dir = TestDir::new("cli_script_cwd");
    let (open, _) = open_in(&dir, &[]);
    assert_eq!(open.paths, vec![logical(&dir)]);
}

#[test]
fn relative_paths_resolve_without_dot_dot() {
    let dir = TestDir::new("cli_script_relative");
    std::fs::create_dir(dir.join("sub")).expect("mkdir");
    std::fs::write(dir.join("top.txt"), b"x").expect("write");
    let (open, _) = open_in(&dir.join("sub"), &["../top.txt"]);
    assert_eq!(open.paths, vec![logical(&dir).join("top.txt")]);
    let (open, _) = open_in(&dir.join("sub"), &[".."]);
    assert_eq!(open.paths, vec![logical(&dir)]);
}

#[test]
fn the_flags_pick_the_pane_and_the_tab() {
    let dir = TestDir::new("cli_script_flags");
    let (open, _) = open_in(&dir, &["--right", "--new-tab", "."]);
    assert_eq!(
        open.target,
        GoToTarget {
            pane: PaneChoice::Right,
            new_tab: true
        }
    );
    let (open, _) = open_in(&dir, &["--left"]);
    assert_eq!(open.target.pane, PaneChoice::Left);
}

#[test]
fn left_and_right_together_are_refused() {
    let dir = TestDir::new("cli_script_both_panes");
    let output = run_in(&dir, &["--print-url", "--left", "--right"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn an_unknown_option_is_refused() {
    let dir = TestDir::new("cli_script_unknown_option");
    let output = run_in(&dir, &["--print-url", "--bogus"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--bogus"));
}

#[test]
fn a_missing_path_is_refused_with_its_name() {
    let dir = TestDir::new("cli_script_missing");
    let output = run_in(&dir, &["--print-url", "not-here.txt"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not-here.txt"));
    assert!(output.stdout.is_empty(), "nothing is sent for a path that isn't there");
}

#[test]
fn a_double_dash_lets_a_name_start_with_a_dash() {
    let dir = TestDir::new("cli_script_dash");
    std::fs::write(dir.join("-v"), b"x").expect("write");
    let (open, _) = open_in(&dir, &["--", "-v"]);
    assert_eq!(open.paths, vec![logical(&dir).join("-v")]);
}

#[test]
fn siblings_all_go_and_nothing_is_said() {
    let dir = TestDir::new("cli_script_siblings");
    std::fs::write(dir.join("a.txt"), b"x").expect("write");
    std::fs::write(dir.join("b.txt"), b"x").expect("write");
    std::fs::create_dir(dir.join("sub")).expect("mkdir");
    // A folder next to a file is a sibling too: the app selects it in the same listing.
    let (open, stderr) = open_in(&dir, &["a.txt", "b.txt", "sub"]);
    assert_eq!(open.paths.len(), 3);
    assert!(stderr.is_empty(), "same folder, no warning: {stderr}");
}

#[test]
fn paths_from_another_folder_are_warned_about_but_still_sent() {
    let dir = TestDir::new("cli_script_spread");
    std::fs::create_dir(dir.join("one")).expect("mkdir");
    std::fs::create_dir(dir.join("two")).expect("mkdir");
    std::fs::write(dir.join("one/a.txt"), b"x").expect("write");
    std::fs::write(dir.join("two/b.txt"), b"x").expect("write");
    let (open, stderr) = open_in(&dir, &["one/a.txt", "two/b.txt"]);
    assert_eq!(open.paths.len(), 2, "the app makes the final call, so it gets them all");
    assert!(stderr.contains("one path isn't in"), "{stderr}");

    // A folder first opens only that folder, so anything after it is left out.
    let (_, stderr) = open_in(&dir, &["one", "two", "one/a.txt"]);
    assert!(stderr.contains("2 paths aren't in"), "{stderr}");
}

/// Lay out `<dir>/Cmdr.app` with the script in it and a version in its `Info.plist`.
fn fake_bundle(dir: &Path, version: &str) -> PathBuf {
    let bundle = dir.join("Cmdr.app");
    std::fs::create_dir_all(bundle.join("Contents/Resources/bin")).expect("mkdir");
    std::fs::write(
        bundle.join("Contents/Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#
        ),
    )
    .expect("write plist");
    let bundled = bundle.join("Contents/Resources/bin/cmdr");
    std::fs::copy(script(), &bundled).expect("copy script");
    bundle
}

#[test]
fn the_version_comes_from_the_bundle_even_through_a_symlink() {
    let dir = TestDir::new("cli_script_version");
    let bundle = fake_bundle(&dir, "1.2.3");
    std::fs::create_dir(dir.join("bin")).expect("mkdir");
    let link = dir.join("bin/cmdr");
    std::os::unix::fs::symlink(bundle.join("Contents/Resources/bin/cmdr"), &link).expect("symlink");
    let output = Command::new(&link).arg("--version").output().expect("the script runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Cmdr 1.2.3");
}

#[test]
fn outside_a_bundle_there_is_no_version_to_report() {
    let dir = TestDir::new("cli_script_no_bundle");
    let output = run_in(&dir, &["--version"]);
    assert_eq!(output.status.code(), Some(1));
}
