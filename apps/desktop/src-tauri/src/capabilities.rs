//! Guards over the capability manifests in `capabilities/`.
//!
//! The manifests are plain data Tauri reads at build time, so nothing in the
//! crate references them and a missing permission only shows up as a silent
//! runtime rejection. This module is test-only: it holds the invariants that
//! would otherwise be prose in `capabilities/CLAUDE.md`.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    const START_DRAGGING: &str = "core:window:allow-start-dragging";
    const TOGGLE_MAXIMIZE: &str = "core:window:allow-internal-toggle-maximize";
    /// Permission sets that already bundle `TOGGLE_MAXIMIZE`, so a manifest
    /// carrying one needn't name it (`default.json`, `debug.json`).
    const BUNDLES_TOGGLE_MAXIMIZE: [&str; 2] = ["core:default", "core:window:default"];

    /// Every manifest under `capabilities/`, as `(file name, permission
    /// strings)`. Object-form permissions (scoped `fs` / `opener` grants) carry
    /// their name under `identifier`.
    fn manifests() -> Vec<(String, Vec<String>)> {
        let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut out = Vec::new();

        for entry in fs::read_dir(&dir).expect("capabilities/ is checked in next to Cargo.toml") {
            let path = entry.expect("readdir entry").path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let name = path
                .file_name()
                .expect("a .json path has a file name")
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path).expect("capability manifest is readable");
            let json: serde_json::Value = serde_json::from_str(&text).expect("capability manifest is valid JSON");

            let permissions = json["permissions"]
                .as_array()
                .expect("every manifest has a permissions array")
                .iter()
                .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()).map(str::to_owned))
                .collect();
            out.push((name, permissions));
        }

        assert!(!out.is_empty(), "found no capability manifests to check");
        out
    }

    /// Tauri's injected `drag.js` invokes `start_dragging` on a single click in
    /// a `data-tauri-drag-region` and `internal_toggle_maximize` on a
    /// double-click, so the two permissions are one pair. Granting only the
    /// first leaves double-click-to-zoom dead AND, because that invoke lives in
    /// Tauri's own script where we can't `try/catch` it, turns an ordinary
    /// gesture into an `FE:uncaught` that auto-sends an error report
    /// (`ERR-ADEAR` on 0.42.0). Rationale: `capabilities/DETAILS.md`.
    #[test]
    fn a_window_that_can_drag_its_title_bar_can_also_zoom_it() {
        let offenders: Vec<String> = manifests()
            .into_iter()
            .filter(|(_, perms)| {
                let can_drag = perms.iter().any(|p| p == START_DRAGGING);
                let can_zoom = perms
                    .iter()
                    .any(|p| p == TOGGLE_MAXIMIZE || BUNDLES_TOGGLE_MAXIMIZE.contains(&p.as_str()));
                can_drag && !can_zoom
            })
            .map(|(name, _)| name)
            .collect();

        assert!(
            offenders.is_empty(),
            "these capability manifests grant {START_DRAGGING} without {TOGGLE_MAXIMIZE}, \
             so double-clicking the title bar rejects and auto-sends an error report: {offenders:?}"
        );
    }
}
