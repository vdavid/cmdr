//! macOS: the owner row's lookups. The installed apps are a cached scan of the
//! app folders (rebuilt when `open_with`'s observer sees the set of apps change,
//! or after `MAX_AGE`), an id elsewhere on this disk is one LaunchServices call,
//! and the receipts are one `pkgutil --file-info` under a deadline.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_about::fact::UnknownWhy;
use cmdr_about::owner::KnownApp;

use crate::file_system::open_with::{
    apps_changed_generation, installed_app_path, read_app_display_name, read_bundle_identifier,
};
use crate::ignore_poison::IgnorePoison;

/// A fallback for an install the observer can't see (an app dragged into
/// `/Applications` launches nothing), as `open_with`'s own cache has.
const MAX_AGE: Duration = Duration::from_secs(30 * 60);

/// How long `pkgutil --file-info` may take: about 20 ms on a warm Mac (macOS
/// 27.0.1, 2026-11-13), so this only bounds a wedged receipts database, inside
/// the row's own budget.
const RECEIPT_BUDGET: Duration = Duration::from_millis(800);

struct Scan {
    apps: Arc<Vec<KnownApp>>,
    generation: u64,
    at: Instant,
}

static SCAN: LazyLock<Mutex<Option<Scan>>> = LazyLock::new(|| Mutex::new(None));

/// The app folders "installed" counts in: top level only. ❌ Never another
/// volume's: a backup disk's or a mounted disk image's app isn't installed.
fn app_folders() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = [
        "/Applications",
        "/Applications/Utilities",
        "/System/Applications",
        "/System/Applications/Utilities",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = dirs::home_dir() {
        folders.push(home.join("Applications"));
    }
    folders
}

pub(crate) fn installed_apps() -> Vec<KnownApp> {
    let generation = apps_changed_generation();
    {
        let scan = SCAN.lock_ignore_poison();
        if let Some(scan) = scan.as_ref()
            && scan.generation == generation
            && scan.at.elapsed() < MAX_AGE
        {
            return scan.apps.as_ref().clone();
        }
    }
    // Scanned outside the lock: two cards racing a cold cache each scan once,
    // which beats one waiting on the other's framework calls.
    let apps = Arc::new(scan_app_folders(&app_folders()));
    *SCAN.lock_ignore_poison() = Some(Scan {
        apps: Arc::clone(&apps),
        generation,
        at: Instant::now(),
    });
    apps.as_ref().clone()
}

/// Every `.app` directly in `folders`, named the way Finder shows it.
pub(crate) fn scan_app_folders(folders: &[PathBuf]) -> Vec<KnownApp> {
    let mut apps = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_app = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"));
            if is_app && path.is_dir() {
                apps.push(known_app(&path));
            }
        }
    }
    apps
}

fn known_app(path: &Path) -> KnownApp {
    let mut other_names = Vec::new();
    if let Some(stem) = path.file_stem() {
        other_names.push(stem.to_string_lossy().into_owned());
    }
    if let Some(name) = bundle_name(path) {
        other_names.push(name);
    }
    KnownApp {
        name: read_app_display_name(path),
        id: read_bundle_identifier(path),
        other_names,
    }
}

/// `CFBundleName`, which can differ from what Finder shows (VS Code's is `Code`).
fn bundle_name(app: &Path) -> Option<String> {
    let plist = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    plist
        .as_dictionary()?
        .get("CFBundleName")?
        .as_string()
        .map(str::to_string)
}

/// The app LaunchServices knows by `id`, when it sits on this computer's own disk.
/// ❌ Never one on another volume or in the Trash: LaunchServices also answers an
/// app on a backup disk, a mounted disk image, or one thrown away.
pub(crate) fn app_with_id(id: &str) -> Option<KnownApp> {
    let path = installed_app_path(id)?;
    if !counts_as_on_this_disk(&path) {
        return None;
    }
    Some(KnownApp {
        name: read_app_display_name(&path),
        id: Some(id.to_string()),
        other_names: Vec::new(),
    })
}

/// Whether an app at `path` is on this computer's own disk and not thrown away.
pub(crate) fn counts_as_on_this_disk(path: &Path) -> bool {
    !path.starts_with("/Volumes") && !path.components().any(|part| part.as_os_str() == ".Trash")
}

/// The package ids of the receipts that list `path`.
pub(crate) fn receipts(path: &str) -> Result<Vec<String>, UnknownWhy> {
    let mut command = crate::subprocess::command_in_c_locale("pkgutil");
    // `pkgutil` has no `--`; the path is absolute, so it can't read as a flag.
    command.arg("--file-info").arg(path);
    let output = tauri::async_runtime::block_on(crate::subprocess::output_within(
        "pkgutil --file-info",
        RECEIPT_BUDGET,
        &mut command,
    ));
    match output {
        Ok(output) if output.status.success() => Ok(package_ids(&String::from_utf8_lossy(&output.stdout))),
        Ok(_) => Err(UnknownWhy::Unparseable),
        Err(crate::subprocess::SubprocessError::TimedOut { .. }) => Err(UnknownWhy::TimedOut),
        Err(crate::subprocess::SubprocessError::Spawn(_)) => Err(UnknownWhy::Unparseable),
    }
}

/// The `pkgid:` fields of `pkgutil --file-info`'s report, one block per receipt
/// that lists the path (verified on macOS 27.0.1, 2026-11-13: a path no receipt
/// lists answers `volume:` and `path:` only, exit 0). Structured output read in
/// the C locale, ❌ never an error's wording.
pub(crate) fn package_ids(report: &str) -> Vec<String> {
    report
        .lines()
        .filter_map(|line| line.strip_prefix("pkgid: "))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn a_report_names_each_receipts_package_id() {
        let report = "volume: /\npath: /Library/Fonts\n\npkgid: com.apple.files.data-template\npkg-version: 27.0.1\n";
        assert_eq!(package_ids(report), ["com.apple.files.data-template"]);
        assert!(package_ids("volume: /\npath: /usr/local/bin/nothing\n").is_empty());
    }

    #[test]
    fn a_path_no_receipt_lists_has_none_and_apples_own_folder_has_apples() {
        assert_eq!(receipts("/usr/local/bin/no-receipt-lists-this-xyz"), Ok(Vec::new()));
        let apples = receipts("/Library/Application Support/Apple").expect("pkgutil answers");
        assert!(
            apples.iter().all(|id| id.starts_with("com.apple.")),
            "every receipt for Apple's own folder is Apple's: {apples:?}"
        );
    }

    #[test]
    fn the_scan_finds_apps_at_the_top_of_each_folder_only() {
        let dir = TestDir::new("about-app-scan");
        let folder = dir.join("Applications");
        let app = folder.join("Tool.app/Contents");
        std::fs::create_dir_all(&app).expect("an app's folders");
        std::fs::write(
            app.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.example.tool</string><key>CFBundleName</key><string>ToolKit</string></dict></plist>"#,
        )
        .expect("an Info.plist");
        std::fs::create_dir_all(folder.join("Vendor/Nested.app")).expect("a nested app");
        let apps = scan_app_folders(&[folder]);
        assert_eq!(apps.len(), 1, "a nested app isn't at the top: {apps:?}");
        assert_eq!(apps[0].id.as_deref(), Some("com.example.tool"));
        assert!(apps[0].other_names.contains(&"ToolKit".to_string()));
        assert!(apps[0].other_names.contains(&"Tool".to_string()));
    }

    #[test]
    fn an_app_on_another_volume_or_in_the_trash_isnt_on_this_disk() {
        assert!(counts_as_on_this_disk(Path::new(
            "/Library/Application Support/X/Helper.app"
        )));
        assert!(!counts_as_on_this_disk(Path::new(
            "/Volumes/Backup/Applications/Tool.app"
        )));
        assert!(!counts_as_on_this_disk(Path::new("/Users/jane/.Trash/Tool.app")));
    }
}
