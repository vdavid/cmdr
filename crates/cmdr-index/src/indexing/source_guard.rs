//! The shared half of the guards that read this crate's own source.
//!
//! Some invariants here can't be held by a type or caught by exercising the code: "every
//! whole-volume holder runs the rescan it owes", "every manager extraction says what a
//! teardown in the window does". What those have in common is that a NEW call site is the
//! regression, so the test has to enumerate call sites rather than run them. Each such guard
//! reads the crate's sources and asserts over what it finds.
//!
//! ❌ Don't re-derive the walk per guard. Two copies of it existed for one commit and the
//! duplication check caught them; this is where the third goes.
//!
//! ⚠️ These guards are a last resort, not a pattern to reach for. A guard that scans text
//! can only ever say "somebody wrote the words", so prefer making the mistake unrepresentable
//! in a type. Reach for this only when the regression is "a new site appeared", which no type
//! can see.

use std::path::{Path, PathBuf};

/// Every non-test `.rs` file under `src/indexing`, as `(path relative to `indexing`, absolute path)`.
///
/// Skips `tests/` directories and `*tests.rs` files: a guard asks what the PRODUCTION code
/// does, and a test naming the thing it tests would answer for itself.
pub(crate) fn indexing_sources() -> Vec<(String, PathBuf)> {
    fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
        for entry in std::fs::read_dir(dir).expect("an indexing dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().expect("file name").to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                if name != "tests" {
                    collect(&path, &rel, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") && !name.ends_with("tests.rs") {
                out.push((rel, path));
            }
        }
    }

    let indexing = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indexing");
    let mut sources = Vec::new();
    collect(&indexing, "", &mut sources);
    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_finds_production_sources_and_skips_the_tests_beside_them() {
        let sources = indexing_sources();
        let names: Vec<&str> = sources.iter().map(|(rel, _)| rel.as_str()).collect();

        assert!(
            names.contains(&"lifecycle/state.rs"),
            "a production file the guards read"
        );
        assert!(
            !names.iter().any(|n| n.ends_with("tests.rs")),
            "a guard asks what production does, so `*tests.rs` stays out: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("tests/") || n.contains("/tests/")),
            "and so does everything under a directory named exactly `tests`: {names:?}"
        );
        // ⚠️ Only a directory named exactly `tests` is skipped, ❌ not every directory whose
        // name ends in it: `lifecycle/cover/cold_drive_tests/` is in, and the guards want it
        // in — they read what the crate's own fixtures do as well as what production does.
        assert!(
            names.iter().any(|n| n.contains("cold_drive_tests/")),
            "a `*_tests` directory is not a `tests` directory: {names:?}"
        );
    }
}
