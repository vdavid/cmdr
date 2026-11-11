//! What every webview learns about the platform before any of its scripts run: which OS, and what
//! it can do. `tauri_builder.rs` registers it as a plugin's init script, which Tauri runs in every
//! webview the app creates. See `apps/desktop/src/lib/platform/DETAILS.md`.

use super::{PlatformCapabilities, capabilities};

/// The platform the frontend's `#lib/platform` reports: Rust's own name for the target OS, which is
/// one of the three values its `Platform` type allows (`macos`, `linux`, `windows`) because the
/// crate root refuses every other target (`cmdr_host`, `docs/guides/platform-seams.md`).
const FRONTEND_PLATFORM: &str = std::env::consts::OS;

/// Defines `window.__CMDR_PLATFORM__` and `window.__CMDR_CAPABILITIES__`, both read-only (and the
/// capabilities frozen), so nothing in the page can change its mind later.
pub(crate) fn init_script() -> String {
    script_for(FRONTEND_PLATFORM, &capabilities())
}

fn script_for(platform: &str, capabilities: &PlatformCapabilities) -> String {
    // A struct of bools always serializes; the JSON is a valid JS object literal as is.
    let capabilities = serde_json::to_string(capabilities).expect("a struct of bools serializes");
    format!(
        "Object.defineProperty(window, '__CMDR_PLATFORM__', {{ value: '{platform}' }});\
         Object.defineProperty(window, '__CMDR_CAPABILITIES__', {{ value: Object.freeze({capabilities}) }});"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_running_os() {
        assert!(["macos", "linux", "windows"].contains(&FRONTEND_PLATFORM));
        assert_eq!(FRONTEND_PLATFORM, std::env::consts::OS);
        assert!(init_script().starts_with(&format!(
            "Object.defineProperty(window, '__CMDR_PLATFORM__', {{ value: '{}' }});",
            std::env::consts::OS
        )));
    }

    #[test]
    fn hands_over_this_os_capabilities_in_the_frontend_spelling() {
        let script = init_script();
        let json = serde_json::to_string(&capabilities()).expect("serializes");
        assert!(script.ends_with(&format!(
            "Object.defineProperty(window, '__CMDR_CAPABILITIES__', {{ value: Object.freeze({json}) }});"
        )));
        // camelCase on the wire, as the generated TS type spells it.
        assert!(json.contains("\"quickLook\":"));
        assert!(json.contains("\"overlayTitleBar\":"));
        assert!(!json.contains('_'));
    }
}
