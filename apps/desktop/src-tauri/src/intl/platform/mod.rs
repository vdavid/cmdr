//! Where each OS keeps the user's language, for the UI half of `intl`. A platform seam
//! (`docs/guides/platform-seams.md`); the resolver it feeds (`../resolve.rs`) is portable.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

/// The POSIX locale environment, read by every arm whose OS has no preference list of its own.
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod locale_env;

const _: fn() -> Option<String> = webview_ui_locale;
const _: fn() -> String = os_ui_locale;
const _: super::native_strings::NativeOs = NATIVE_TEXT_OS;

/// Which OS's text a native string reads when its catalog value is a platform block.
/// `native_strings::pick` does the choosing; only this answer is per-OS.
pub(crate) use imp::NATIVE_TEXT_OS;

/// The catalog the OS asks the WEBVIEW to open, or `None` when the OS has no answer of its own
/// and the webview's default should stand. Read fresh on every call, so a live change shows.
pub(crate) use imp::webview_ui_locale;

/// The catalog the NATIVE surfaces (the menu bar, the already-running alert) open while the user
/// hasn't pinned a language. Always an answer: they have no webview to defer to.
pub(crate) use imp::os_ui_locale;

#[cfg(test)]
mod tests {
    use super::super::native_strings::NativeOs;

    #[test]
    fn this_hosts_arm_answers_its_own_os_for_native_text() {
        // Each CI OS lane runs this against its own arm, Windows included: an arm copied from
        // its neighbour without changing the answer would have Windows read Linux words.
        let host = [
            (cfg!(target_os = "macos"), NativeOs::MacOs),
            (cfg!(target_os = "windows"), NativeOs::Windows),
            (cfg!(target_os = "linux"), NativeOs::Linux),
        ]
        .into_iter()
        .find_map(|(is_host, os)| is_host.then_some(os))
        .expect("the app builds only for macOS, Windows, and Linux");
        assert_eq!(super::NATIVE_TEXT_OS, host);
    }
}
