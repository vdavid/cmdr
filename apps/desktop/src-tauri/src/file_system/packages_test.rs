use super::*;

#[test]
fn only_unlisted_dotted_names_are_asked() {
    let asked = lookup_candidates(vec![
        "/a/Logo.sketchbundle".to_string(),
        "/a/release.v1.2".to_string(),
        "/a/Cmdr.app".to_string(),
        "/a/Documents".to_string(),
        "/a/.config".to_string(),
    ]);
    assert_eq!(asked, ["/a/Logo.sketchbundle", "/a/release.v1.2"]);
}

#[cfg(target_os = "macos")]
mod macos {
    use super::super::platform::is_package_to_macos;
    use crate::test_support::TestDir;

    /// `kHasBundle` in the Finder flags (`<CarbonCore/Finder.h>`): the bit that
    /// makes ANY folder a package, whatever its name. Big-endian at offset 8 of
    /// `com.apple.FinderInfo`.
    const K_HAS_BUNDLE: u16 = 0x2000;

    fn finder_info_with_flags(flags: u16) -> Vec<u8> {
        let mut info = vec![0u8; 32];
        info[8..10].copy_from_slice(&flags.to_be_bytes());
        info
    }

    #[test]
    fn a_plain_dotted_folder_is_not_a_package() {
        let dir = TestDir::new("pkg_plain");
        let folder = dir.join("release.v1.2");
        std::fs::create_dir(&folder).expect("create folder");
        assert!(!is_package_to_macos(&folder.to_string_lossy()));
    }

    #[test]
    fn a_registered_package_type_is_one() {
        let dir = TestDir::new("pkg_app");
        let folder = dir.join("Thing.app");
        std::fs::create_dir(&folder).expect("create folder");
        assert!(is_package_to_macos(&folder.to_string_lossy()));
    }

    /// The case the list can't see: a folder whose name means nothing to it,
    /// carrying the bundle bit.
    #[test]
    fn a_folder_with_the_bundle_bit_is_one_whatever_its_name() {
        let dir = TestDir::new("pkg_bit");
        let folder = dir.join("Logo.sketchbundle");
        std::fs::create_dir(&folder).expect("create folder");
        xattr::set(&folder, "com.apple.FinderInfo", &finder_info_with_flags(K_HAS_BUNDLE)).expect("set FinderInfo");
        assert!(is_package_to_macos(&folder.to_string_lossy()));
    }

    #[test]
    fn a_missing_path_or_a_file_is_not_a_package() {
        let dir = TestDir::new("pkg_missing");
        assert!(!is_package_to_macos(&dir.join("gone.bundle2").to_string_lossy()));
        let file = dir.join("notes.sketchbundle");
        std::fs::write(&file, b"x").expect("write file");
        assert!(!is_package_to_macos(&file.to_string_lossy()));
    }
}
