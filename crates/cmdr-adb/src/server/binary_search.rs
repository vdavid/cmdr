//! Where each OS keeps `adb`, as data: the binary's file name, Android Studio's default SDK, and
//! the package-manager folders worth a look. Pure and compiled on every OS, so each OS's list is
//! tested on every OS; `platform/` picks this OS's [`AdbSearch`] (`HOST_SEARCH`).

use std::path::PathBuf;

/// The folder an OS's default SDK sits under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SdkBase {
    /// The user's home folder (`~`).
    Home,
    /// `%LOCALAPPDATA%` on Windows.
    LocalAppData,
}

/// One OS's answer to "where might `adb` be?", after `$PATH`, `$ANDROID_HOME`, and
/// `$ANDROID_SDK_ROOT`, which every OS asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdbSearch {
    /// The binary's file name.
    pub(crate) exe: &'static str,
    /// Android Studio's default SDK, as its base folder and the path under it.
    pub(crate) default_sdk: (SdkBase, &'static str),
    /// Folders package managers install into that may not be on `$PATH` for a GUI app.
    pub(crate) fixed_dirs: &'static [&'static str],
}

#[allow(
    dead_code,
    reason = "each OS's build names its own in `platform/`; the tests read all three"
)]
impl AdbSearch {
    /// Android Studio's `~/Library/Android/sdk`, then Homebrew's two prefixes (a Dock-launched
    /// app gets launchd's short `PATH`, without either).
    pub(crate) const MACOS: AdbSearch = AdbSearch {
        exe: "adb",
        default_sdk: (SdkBase::Home, "Library/Android/sdk"),
        fixed_dirs: &["/opt/homebrew/bin", "/usr/local/bin"],
    };

    /// Android Studio's `~/Android/Sdk`, then `/usr/local/bin`. A distro's own `adb` lives in
    /// `/usr/bin`, which every `PATH` carries.
    pub(crate) const LINUX: AdbSearch = AdbSearch {
        exe: "adb",
        default_sdk: (SdkBase::Home, "Android/Sdk"),
        fixed_dirs: &["/usr/local/bin"],
    };

    /// `adb.exe`, and Android Studio's `%LOCALAPPDATA%\Android\Sdk`. Scoop and Chocolatey put
    /// their shims on `PATH`, so no fixed folder is needed.
    pub(crate) const WINDOWS: AdbSearch = AdbSearch {
        exe: "adb.exe",
        default_sdk: (SdkBase::LocalAppData, "Android/Sdk"),
        fixed_dirs: &[],
    };
}

/// What the search reads from the process, read once by the caller so the list stays pure.
#[derive(Debug, Clone, Default)]
pub(crate) struct SearchEnv {
    /// `$PATH`, split by this OS's rule.
    pub(crate) path_dirs: Vec<PathBuf>,
    pub(crate) android_home: Option<PathBuf>,
    pub(crate) android_sdk_root: Option<PathBuf>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) local_app_data: Option<PathBuf>,
}

impl SearchEnv {
    /// This process's environment.
    pub(crate) fn of_process() -> Self {
        SearchEnv {
            path_dirs: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            android_home: std::env::var_os("ANDROID_HOME").map(PathBuf::from),
            android_sdk_root: std::env::var_os("ANDROID_SDK_ROOT").map(PathBuf::from),
            home: std::env::home_dir(),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        }
    }
}

/// Every place `search` says `adb` might be, in the order they're tried: `$PATH`,
/// `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`, the default SDK's
/// `platform-tools`, then the fixed folders. A missing variable drops only its own entries.
pub(crate) fn candidates(search: &AdbSearch, env: &SearchEnv) -> Vec<PathBuf> {
    let exe = search.exe;
    let mut out: Vec<PathBuf> = env.path_dirs.iter().map(|dir| dir.join(exe)).collect();
    for root in [&env.android_home, &env.android_sdk_root].into_iter().flatten() {
        out.push(root.join("platform-tools").join(exe));
    }
    let (base, sdk) = search.default_sdk;
    let base_dir = match base {
        SdkBase::Home => &env.home,
        SdkBase::LocalAppData => &env.local_app_data,
    };
    if let Some(base_dir) = base_dir {
        // One component at a time, so a Windows path gets Windows separators throughout.
        let sdk_dir = sdk.split('/').fold(base_dir.clone(), |dir, name| dir.join(name));
        out.push(sdk_dir.join("platform-tools").join(exe));
    }
    out.extend(search.fixed_dirs.iter().map(|dir| PathBuf::from(dir).join(exe)));
    out
}

#[cfg(test)]
#[path = "binary_search_test.rs"]
mod tests;
