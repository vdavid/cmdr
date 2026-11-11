//! Guards over the Tauri config split: `tauri.conf.json` plus one `tauri.<os>.conf.json` per
//! platform, which Tauri merges as a JSON Merge Patch (RFC 7396). This module is test-only: the
//! files are data Tauri reads at build time. Layout and why: `DETAILS.md` § "Tauri config per
//! platform".

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value};

    const SHARED: &str = include_str!("../tauri.conf.json");
    const MACOS: &str = include_str!("../tauri.macos.conf.json");
    const LINUX: &str = include_str!("../tauri.linux.conf.json");
    const WINDOWS: &str = include_str!("../tauri.windows.conf.json");

    /// The main-window keys only macOS reads: tauri-runtime-wry applies them under
    /// `cfg(target_os = "macos")`, and wry reads `acceptFirstMouse` only in its WKWebView.
    const MACOS_ONLY_WINDOW_KEYS: [&str; 4] = [
        "titleBarStyle",
        "trafficLightPosition",
        "hiddenTitle",
        "acceptFirstMouse",
    ];

    fn parse(source: &str) -> Value {
        serde_json::from_str(source).expect("a Tauri config file is JSON")
    }

    fn main_window(config: &Value) -> &Map<String, Value> {
        config["app"]["windows"][0]
            .as_object()
            .expect("the config names the main window first in `app.windows`")
    }

    /// A merge patch REPLACES arrays, so the macOS file has to restate the whole main window.
    /// This keeps that copy equal to the shared one apart from the macOS chrome.
    #[test]
    fn the_macos_main_window_is_the_shared_one_plus_only_its_chrome() {
        let shared = parse(SHARED);
        let macos = parse(MACOS);
        assert_eq!(
            shared["app"]["windows"].as_array().map(Vec::len),
            Some(1),
            "one main window in tauri.conf.json"
        );
        assert_eq!(
            macos["app"]["windows"].as_array().map(Vec::len),
            Some(1),
            "one main window in tauri.macos.conf.json"
        );

        let mut macos_window = main_window(&macos).clone();
        for key in MACOS_ONLY_WINDOW_KEYS {
            assert!(
                macos_window.remove(key).is_some(),
                "tauri.macos.conf.json's main window sets `{key}`"
            );
            assert!(
                !main_window(&shared).contains_key(key),
                "`{key}` is macOS-only, so it belongs in tauri.macos.conf.json, not tauri.conf.json"
            );
        }
        assert_eq!(
            &macos_window,
            main_window(&shared),
            "tauri.macos.conf.json's main window drifted from tauri.conf.json's"
        );
    }

    /// Off macOS nothing restates the window list, so Linux and Windows get the shared window.
    #[test]
    fn only_macos_overrides_the_window_list() {
        for (file, source) in [("tauri.linux.conf.json", LINUX), ("tauri.windows.conf.json", WINDOWS)] {
            assert!(
                parse(source)["app"]["windows"].is_null(),
                "{file} must not restate `app.windows`"
            );
        }
    }

    /// Each platform names its own installers, and the shared file names none.
    #[test]
    fn every_platform_lists_its_bundle_targets() {
        assert!(
            parse(SHARED)["bundle"]["targets"].is_null(),
            "`bundle.targets` is per platform, never in tauri.conf.json"
        );
        for (file, source) in [
            ("tauri.macos.conf.json", MACOS),
            ("tauri.linux.conf.json", LINUX),
            ("tauri.windows.conf.json", WINDOWS),
        ] {
            assert!(
                parse(source)["bundle"]["targets"].is_array(),
                "{file} must list `bundle.targets`"
            );
        }
    }

    /// tauri-build fails the build when the `tauri` dependency's features disagree with the
    /// effective config's (`tauri-build` `manifest::check`), and Cargo.toml turns
    /// `macos-private-api` on for every target. So the key stays shared.
    #[test]
    fn macos_private_api_stays_in_the_shared_config() {
        assert_eq!(parse(SHARED)["app"]["macOSPrivateApi"], Value::Bool(true));
        for (file, source) in [
            ("tauri.macos.conf.json", MACOS),
            ("tauri.linux.conf.json", LINUX),
            ("tauri.windows.conf.json", WINDOWS),
        ] {
            assert!(
                parse(source)["app"]["macOSPrivateApi"].is_null(),
                "{file} must not set `macOSPrivateApi`"
            );
        }
    }
}
