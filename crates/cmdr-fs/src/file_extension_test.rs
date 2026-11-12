use super::*;
use crate::archive_format::{ArchiveFormat, format_for_name};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    package_extensions: Vec<String>,
    compound_extensions: Vec<String>,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum SpecKind {
    File,
    Folder,
    Package,
}

impl From<&SpecKind> for NameKind {
    fn from(kind: &SpecKind) -> Self {
        match kind {
            SpecKind::File => NameKind::File,
            SpecKind::Folder => NameKind::Folder,
            SpecKind::Package => NameKind::Package,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    kind: SpecKind,
    extension: String,
}

/// The rule's spec, shared with the frontend's `file-extension.test.ts`, which
/// runs the same cases against its mirror. Both suites reading ONE file is what
/// keeps the two languages agreeing.
fn spec() -> Spec {
    serde_json::from_str(include_str!("file_extension_cases.json")).expect("the shared cases parse")
}

#[test]
fn every_shared_case_holds() {
    for case in spec().cases {
        let kind = NameKind::from(&case.kind);
        let got = extension_of(&case.name, kind).unwrap_or("");
        assert_eq!(
            got, case.extension,
            "{:?} ({kind:?}) should have extension {:?}",
            case.name, case.extension
        );
    }
}

/// The frontend keeps a copy for names no entry describes yet (a folder's new
/// name in the rename field); both sides compare theirs with this one.
#[test]
fn the_package_list_is_the_shared_one() {
    let mut ours: Vec<&str> = PACKAGE_EXTENSIONS.to_vec();
    ours.sort_unstable();
    let mut shared = spec().package_extensions;
    shared.sort_unstable();
    assert_eq!(ours, shared);
}

#[test]
fn the_compound_list_is_the_shared_one() {
    assert_eq!(COMPOUND_EXTENSIONS.to_vec(), spec().compound_extensions);
}

/// The compounds are exactly the compressed tars `archive_format` names with a
/// dotted label, so an archive the app browses and the extension the rename
/// field selects can't disagree on where the name ends.
#[test]
fn the_compounds_are_the_dotted_archive_labels() {
    for compound in COMPOUND_EXTENSIONS {
        let format = format_for_name(&format!("a.{compound}")).expect("a compound names an archive");
        assert!(matches!(format, ArchiveFormat::Tar(_)), "{compound} is a tar");
        assert_eq!(format.label(), *compound);
    }
}

#[test]
fn every_listed_suffix_is_a_package_suffix() {
    for ext in PACKAGE_EXTENSIONS {
        assert!(has_package_suffix(&format!("Thing.{ext}")), "{ext}");
        assert!(
            has_package_suffix(&format!("Thing.{}", ext.to_uppercase())),
            "{ext} uppercased"
        );
    }
}

#[test]
fn a_package_suffix_needs_a_stem_and_a_listed_suffix() {
    assert!(!has_package_suffix(".app"));
    assert!(!has_package_suffix("Documents"));
    assert!(!has_package_suffix("folder.with.dots"));
    assert!(!has_package_suffix("archive.zip"));
    assert!(!has_package_suffix("Thing.app.old"));
}

/// Packages are a macOS concept: on Linux and Windows a folder named like one
/// is a plain folder (no `pkg:` icon, no extension-change ask, no bundle Enter
/// policy).
#[test]
fn only_macos_reads_a_suffixed_folder_name_as_a_package() {
    let packages_here = cfg!(any(target_os = "macos", target_family = "wasm"));
    assert_eq!(folder_names_make_packages(), packages_here);
    assert_eq!(is_listed_package("Cmdr.app"), packages_here);
    let kind = if packages_here {
        NameKind::Package
    } else {
        NameKind::Folder
    };
    assert_eq!(NameKind::by_name("Cmdr.app", true), kind);
}

#[test]
fn by_name_reads_the_list_and_only_for_folders() {
    if folder_names_make_packages() {
        assert_eq!(NameKind::by_name("Cmdr.app", true), NameKind::Package);
    }
    assert_eq!(NameKind::by_name("Cmdr.app", false), NameKind::File);
    assert_eq!(NameKind::by_name("release.v1.2", true), NameKind::Folder);
    assert_eq!(NameKind::by_name("Sketch.myBundle", true), NameKind::Folder);
}

#[test]
fn of_trusts_the_entry_over_the_name() {
    assert_eq!(NameKind::of(true, true), NameKind::Package);
    assert_eq!(NameKind::of(true, false), NameKind::Folder);
    assert_eq!(NameKind::of(false, true), NameKind::File);
}

#[test]
fn only_an_unlisted_dotted_folder_needs_macos() {
    assert!(needs_package_lookup("Sketch.myBundle"));
    assert!(needs_package_lookup("release.v1.2"));
    assert!(!needs_package_lookup("Cmdr.app"));
    assert!(!needs_package_lookup("Documents"));
    assert!(!needs_package_lookup(".config"));
    assert!(!needs_package_lookup("trailing."));
}

#[test]
fn name_without_extension_strips_the_dot_too() {
    assert_eq!(name_without_extension("report.pdf", NameKind::File), "report");
    assert_eq!(name_without_extension("src.tar.gz", NameKind::File), "src");
    assert_eq!(
        name_without_extension("Smlouva v. 2 final", NameKind::File),
        "Smlouva v. 2 final"
    );
    assert_eq!(name_without_extension("release.v1.2", NameKind::Folder), "release.v1.2");
    assert_eq!(name_without_extension("Cmdr.app", NameKind::Package), "Cmdr");
}

#[test]
fn a_multibyte_tail_never_panics() {
    assert_eq!(extension_of("ünïcödé.tär.gz", NameKind::File), Some("gz"));
    assert_eq!(extension_of("日本語", NameKind::File), None);
    assert_eq!(extension_of("é.gz", NameKind::File), Some("gz"));
    assert_eq!(extension_of("Bündel.pâquet", NameKind::Package), Some("pâquet"));
}
