//! Which folders and files "Delete all Cmdr data" removes, worked out from the roots this
//! instance actually uses. Pure: no filesystem access, so it's tested against made-up roots.

use std::path::{Path, PathBuf};

/// Where this instance keeps its data. Every field comes from the app's own path helpers
/// (`config`, `logging`, the Tauri identifier), so a dev, worktree, or E2E instance plans its
/// own folders and never production's.
#[derive(Debug, Clone)]
pub struct WipeRoots {
    /// The app data dir (`CMDR_DATA_DIR`, else `~/Library/Application Support/<identifier>`).
    pub data_dir: PathBuf,
    /// The cache dir holding the drive index and the bulk-rename Results files
    /// (`config::app_cache_dir`).
    pub cache_dir: Option<PathBuf>,
    /// The log dir this session writes to (`logging::log_dir`).
    pub log_dir: Option<PathBuf>,
    /// `~/Library` on macOS, where AppKit and WebKit keep state named after the identifier.
    /// `None` elsewhere.
    pub library_dir: Option<PathBuf>,
    /// The bundle identifier this instance runs under (instance-suffixed off prod).
    pub identifier: String,
    /// The person's home folder, which no target may ever be or contain.
    pub home_dir: Option<PathBuf>,
}

/// What a target holds, for logs and docs. The dialog speaks about the data, never these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    AppData,
    Cache,
    Logs,
    Preferences,
    SavedState,
    WebKit,
    HttpStorage,
    /// `~/Library/Caches/<identifier>` when the drive index lives elsewhere (an E2E run): WebKit
    /// still keeps its caches there.
    LibraryCache,
    /// `~/Library/Logs/<identifier>` when this session logs elsewhere.
    LibraryLogs,
}

/// One file or folder, removed whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WipeTarget {
    pub kind: TargetKind,
    pub path: PathBuf,
}

/// Everything to remove, in order: the data dir first (it holds the most), then the rest.
/// Unsafe targets (see [`is_safe_target`]) are dropped, and a target inside another whole
/// target is dropped too, since removing the outer one takes it along.
pub fn build_plan(roots: &WipeRoots) -> Vec<WipeTarget> {
    let mut candidates = vec![whole(TargetKind::AppData, roots.data_dir.clone())];
    if let Some(dir) = &roots.cache_dir {
        candidates.push(whole(TargetKind::Cache, dir.clone()));
    }
    if let Some(dir) = &roots.log_dir {
        candidates.push(whole(TargetKind::Logs, dir.clone()));
    }
    if let Some(library) = &roots.library_dir {
        let id = &roots.identifier;
        candidates.extend([
            whole(
                TargetKind::Preferences,
                library.join("Preferences").join(format!("{id}.plist")),
            ),
            whole(
                TargetKind::SavedState,
                library.join("Saved Application State").join(format!("{id}.savedState")),
            ),
            whole(TargetKind::WebKit, library.join("WebKit").join(id)),
            whole(TargetKind::HttpStorage, library.join("HTTPStorages").join(id)),
            whole(TargetKind::LibraryCache, library.join("Caches").join(id)),
            whole(TargetKind::LibraryLogs, library.join("Logs").join(id)),
        ]);
    }
    let safe: Vec<WipeTarget> = candidates
        .into_iter()
        .filter(|target| is_safe_target(&target.path, roots))
        .collect();
    let mut plan: Vec<WipeTarget> = Vec::new();
    for target in &safe {
        let covered = safe
            .iter()
            .any(|outer| outer.path != target.path && target.path.starts_with(&outer.path));
        let duplicate = plan.iter().any(|kept| kept.path == target.path);
        if !covered && !duplicate {
            plan.push(target.clone());
        }
    }
    plan
}

fn whole(kind: TargetKind, path: PathBuf) -> WipeTarget {
    WipeTarget { kind, path }
}

/// A last guard against a misresolved root: the target must be an absolute path at least two
/// folders deep, must not be the home folder or anything containing it, must not be a folder
/// directly in the home folder (`~/Documents`) or in `~/Library` (`~/Library/Caches`), and
/// anything under `~/Library` must carry the identifier in its own name.
pub fn is_safe_target(path: &Path, roots: &WipeRoots) -> bool {
    use std::path::Component;
    if !path.is_absolute() {
        return false;
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    let depth = path.components().filter(|c| matches!(c, Component::Normal(_))).count();
    if depth < 2 {
        return false;
    }
    if let Some(home) = &roots.home_dir
        && (home.starts_with(path) || path.parent() == Some(home.as_path()))
    {
        return false;
    }
    if let Some(library) = &roots.library_dir {
        if library.starts_with(path) || path.parent() == Some(library.as_path()) {
            return false;
        }
        if path.starts_with(library) {
            return !roots.identifier.is_empty()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains(roots.identifier.as_str()));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "com.veszelovszki.cmdr";

    fn home() -> PathBuf {
        PathBuf::from("/Users/someone")
    }

    fn library() -> PathBuf {
        home().join("Library")
    }

    /// A production macOS instance: every root in its default place.
    fn prod_roots() -> WipeRoots {
        WipeRoots {
            data_dir: library().join("Application Support").join(ID),
            cache_dir: Some(library().join("Caches").join(ID)),
            log_dir: Some(library().join("Logs").join(ID)),
            library_dir: Some(library()),
            identifier: ID.to_string(),
            home_dir: Some(home()),
        }
    }

    fn paths(plan: &[WipeTarget]) -> Vec<PathBuf> {
        plan.iter().map(|t| t.path.clone()).collect()
    }

    #[test]
    fn a_production_mac_plans_every_folder_and_file_cmdr_keeps() {
        let plan = build_plan(&prod_roots());
        let lib = library();
        assert_eq!(
            paths(&plan),
            vec![
                lib.join("Application Support").join(ID),
                lib.join("Caches").join(ID),
                lib.join("Logs").join(ID),
                lib.join("Preferences").join(format!("{ID}.plist")),
                lib.join("Saved Application State").join(format!("{ID}.savedState")),
                lib.join("WebKit").join(ID),
                lib.join("HTTPStorages").join(ID),
            ]
        );
    }

    /// The security event log lives in the data dir, so the data dir's target takes it. Named here
    /// so moving the file elsewhere can't silently escape the wipe.
    #[test]
    fn the_security_event_log_goes_with_the_wipe() {
        let roots = prod_roots();
        let log = crate::security_log::file_path(&roots.data_dir);
        assert!(
            build_plan(&roots).iter().any(|t| log.starts_with(&t.path)),
            "{} isn't inside any target",
            log.display()
        );
    }

    /// A worktree dev instance runs under its own identifier and data dir. Every target is that
    /// instance's, never production's.
    #[test]
    fn a_dev_instance_plans_only_its_own_folders() {
        let id = "com.veszelovszki.cmdr-dev-my-slug";
        let lib = library();
        let roots = WipeRoots {
            data_dir: lib.join("Application Support").join(id),
            cache_dir: Some(lib.join("Caches").join(id)),
            log_dir: Some(lib.join("Application Support").join(id).join("logs")),
            library_dir: Some(lib.clone()),
            identifier: id.to_string(),
            home_dir: Some(home()),
        };
        let plan = build_plan(&roots);
        for target in &plan {
            assert!(
                target.path.to_string_lossy().contains(id),
                "{} isn't this instance's",
                target.path.display()
            );
        }
        assert!(
            !plan.iter().any(|t| t.kind == TargetKind::Logs),
            "logs inside the data dir go with it, so they aren't a separate target"
        );
    }

    /// An E2E run keeps its data, cache, and logs in one throwaway folder, so that folder is one
    /// target; the Library state named after its identifier still gets planned.
    #[test]
    fn an_e2e_run_nests_its_cache_and_logs_inside_the_data_dir() {
        let id = "com.veszelovszki.cmdr-e2e-abc";
        let data = PathBuf::from("/private/tmp/cmdr-e2e-data");
        let roots = WipeRoots {
            data_dir: data.clone(),
            cache_dir: Some(data.join("cache")),
            log_dir: Some(data.join("logs")),
            library_dir: Some(library()),
            identifier: id.to_string(),
            home_dir: Some(home()),
        };
        let plan = build_plan(&roots);
        assert_eq!(plan[0].path, data);
        assert!(
            !plan
                .iter()
                .any(|t| t.kind == TargetKind::Cache || t.kind == TargetKind::Logs)
        );
        assert!(plan.iter().any(|t| t.path == library().join("Caches").join(id)));
    }

    #[test]
    fn linux_plans_no_library_state() {
        let roots = WipeRoots {
            data_dir: PathBuf::from("/home/someone/.local/share/com.veszelovszki.cmdr"),
            cache_dir: Some(PathBuf::from("/home/someone/.cache/com.veszelovszki.cmdr")),
            log_dir: Some(PathBuf::from("/home/someone/.local/share/com.veszelovszki.cmdr/logs")),
            library_dir: None,
            identifier: ID.to_string(),
            home_dir: Some(PathBuf::from("/home/someone")),
        };
        assert_eq!(
            paths(&build_plan(&roots)),
            vec![
                PathBuf::from("/home/someone/.local/share/com.veszelovszki.cmdr"),
                PathBuf::from("/home/someone/.cache/com.veszelovszki.cmdr"),
            ]
        );
    }

    /// A data dir that resolved to the home folder, `~/Library`, or a folder right inside either
    /// (an empty env var, a bad join) must never be planned: removing it would take the person's
    /// own files.
    #[test]
    fn a_misresolved_root_is_never_planned() {
        for bad in [
            PathBuf::from("/"),
            PathBuf::from("/Users"),
            home(),
            home().join("Documents"),
            library(),
            library().join("Application Support"),
            library().join("Caches"),
            PathBuf::from("relative/path"),
            library().join("Application Support").join(ID).join(".."),
        ] {
            let mut roots = prod_roots();
            roots.data_dir = bad.clone();
            roots.cache_dir = Some(bad.clone());
            assert!(!is_safe_target(&bad, &roots), "{} passed the guard", bad.display());
            assert!(
                !build_plan(&roots).iter().any(|t| t.path == bad),
                "{} was planned",
                bad.display()
            );
        }
    }

    /// Only Cmdr's own entries in `~/Library`: another app's plist or WebKit folder is never ours.
    #[test]
    fn a_library_target_must_carry_the_identifier() {
        let roots = prod_roots();
        assert!(!is_safe_target(
            &library().join("Preferences").join("com.apple.finder.plist"),
            &roots
        ));
        assert!(is_safe_target(
            &library().join("Preferences").join(format!("{ID}.plist")),
            &roots
        ));
    }
}
