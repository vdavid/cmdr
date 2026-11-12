//! The app-path grammar, driven by the case table the frontend's suite reads too
//! (`path_grammar_cases.json`, against `apps/desktop/src/lib/path/grammar.ts`), so the two
//! languages can't drift apart. A new case goes into that file, ❌ never into one suite.

use super::*;

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
enum Flavor {
    Posix,
    Windows,
}

impl From<Flavor> for PathFlavor {
    fn from(flavor: Flavor) -> Self {
        match flavor {
            Flavor::Posix => PathFlavor::Posix,
            Flavor::Windows => PathFlavor::Windows,
        }
    }
}

#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Posix,
    Drive,
    Unc,
    Scheme,
}

impl From<PathKind> for Kind {
    fn from(kind: PathKind) -> Self {
        match kind {
            PathKind::Posix => Kind::Posix,
            PathKind::Drive => Kind::Drive,
            PathKind::Unc => Kind::Unc,
            PathKind::Scheme => Kind::Scheme,
        }
    }
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Case {
    path: String,
    flavor: Flavor,
    kind: Option<Kind>,
    root: Option<String>,
    normalized: Option<String>,
    is_root: Option<bool>,
    parent: Option<String>,
    basename: Option<String>,
    segments: Option<Vec<String>>,
    shown: String,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UserInputCase {
    text: String,
    flavor: Flavor,
    wire: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    cases: Vec<Case>,
    user_input: Vec<UserInputCase>,
}

fn table() -> Table {
    serde_json::from_str(include_str!("path_grammar_cases.json")).expect("the shared cases parse")
}

#[test]
fn every_shared_case_holds() {
    for case in table().cases {
        let flavor = PathFlavor::from(case.flavor);
        let what = format!("{:?} {:?}", case.flavor, case.path);
        let parsed = AppPath::parse(&case.path, flavor);
        assert_eq!(
            path_kind(&case.path, flavor).map(Kind::from),
            case.kind,
            "kind of {what}"
        );
        assert_eq!(
            parsed.as_ref().map(|p| p.root().to_string()),
            case.root,
            "root of {what}"
        );
        assert_eq!(normalize_path(&case.path, flavor), case.normalized, "normalized {what}");
        assert_eq!(
            parsed.as_ref().map(|p| p.names().to_vec()),
            case.segments,
            "segments of {what}"
        );
        if let Some(is_root) = case.is_root {
            assert_eq!(
                parsed.as_ref().is_some_and(AppPath::is_root),
                is_root,
                "root-ness of {what}"
            );
        }
        assert_eq!(parent_in_volume(&case.path, flavor), case.parent, "parent of {what}");
        assert_eq!(
            basename_in_volume(&case.path, flavor),
            case.basename,
            "basename of {what}"
        );
        assert_eq!(shown_path(&case.path, flavor), case.shown, "shown {what}");
    }
}

#[test]
fn every_shared_user_input_case_holds() {
    for case in table().user_input {
        let flavor = PathFlavor::from(case.flavor);
        assert_eq!(
            from_user_input(&case.text, flavor),
            case.wire,
            "{:?} {:?}",
            case.flavor,
            case.text
        );
    }
}

fn win(text: &str) -> AppPath {
    AppPath::parse(text, PathFlavor::Windows).expect("a Windows app path")
}

#[test]
fn to_native_is_the_long_path_form_on_windows() {
    assert_eq!(win("C:/Users/test_user_1").to_native(), r"\\?\C:\Users\test_user_1");
    assert_eq!(win("C:/").to_native(), r"\\?\C:\");
    assert_eq!(
        win("//nas/media/films/a.mkv").to_native(),
        r"\\?\UNC\nas\media\films\a.mkv"
    );
    assert_eq!(win("//nas/media").to_native(), r"\\?\UNC\nas\media\");
}

#[test]
fn to_native_is_the_wire_spelling_on_posix() {
    let path = AppPath::parse("/Users/test_user_1/a\\b", PathFlavor::Posix).expect("a POSIX path");
    assert_eq!(path.to_native(), "/Users/test_user_1/a\\b");
    assert_eq!(AppPath::parse("/", PathFlavor::Posix).expect("root").to_native(), "/");
}

#[test]
fn from_native_takes_every_windows_spelling_to_one_app_path() {
    for native in [
        r"\\?\C:\Users\test_user_1",
        r"C:\Users\test_user_1",
        "C:/Users/test_user_1",
        r"c:\Users\test_user_1\",
    ] {
        let path = AppPath::from_native(native, PathFlavor::Windows).expect(native);
        assert_eq!(path.to_string(), "C:/Users/test_user_1", "{native}");
    }
    for native in [r"\\?\UNC\nas\media\x", r"\\nas\media\x", "//nas/media/x"] {
        let path = AppPath::from_native(native, PathFlavor::Windows).expect(native);
        assert_eq!(path.to_string(), "//nas/media/x", "{native}");
    }
}

#[test]
fn from_native_refuses_what_names_no_folder() {
    for native in [
        r"\\.\PhysicalDrive0",
        r"\\?\Volume{0b1c2d3e-0000-0000-0000-100000000000}\",
        "Users",
        r"\x",
        "",
    ] {
        assert_eq!(AppPath::from_native(native, PathFlavor::Windows), None, "{native}");
    }
}

#[test]
fn native_round_trips() {
    for wire in ["C:/", "C:/Users/test_user_1", "//nas/media/", "//nas/media/a/b"] {
        let path = win(wire);
        assert_eq!(
            AppPath::from_native(&path.to_native(), PathFlavor::Windows),
            Some(path),
            "{wire}"
        );
    }
}

#[test]
fn join_and_parent_never_leave_the_volume() {
    let root = win("C:/");
    assert_eq!(root.parent(), None);
    assert_eq!(root.join("../../Windows").to_string(), "C:/Windows");
    assert_eq!(win("//nas/media/x").join("../..").to_string(), "//nas/media/");
    let posix = AppPath::parse("/Users", PathFlavor::Posix).expect("a POSIX path");
    assert_eq!(
        posix.join("test_user_1/./notes.txt").to_string(),
        "/Users/test_user_1/notes.txt"
    );
    assert_eq!(posix.parent().map(|p| p.to_string()), Some("/".to_string()));
}

#[test]
fn starts_with_matches_whole_components_on_one_root() {
    assert!(win("C:/Users/test_user_1").starts_with(&win("C:/Users")));
    assert!(win("C:/Users").starts_with(&win("C:/")));
    assert!(!win("C:/Users-1").starts_with(&win("C:/Users")));
    assert!(!win("D:/Users").starts_with(&win("C:/")));
    assert!(!win("//nas/media-1/x").starts_with(&win("//nas/media")));
}

#[test]
fn the_host_flavor_follows_the_build_target() {
    // The standard library's own per-target answer, so the test names no OS itself.
    let expected = if std::path::MAIN_SEPARATOR == '\\' {
        PathFlavor::Windows
    } else {
        PathFlavor::Posix
    };
    assert_eq!(PathFlavor::HOST, expected);
}
