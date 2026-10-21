fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Only where the fetch writes it: on any other host `download-llama-server.go` lays down a
    // placeholder and no `.version`, and cargo reads a watched path that doesn't exist as
    // "changed" on EVERY invocation. That re-ran this script and recompiled `cmdr` for each cargo
    // call on Linux, 1m40s per test lane on CI (#619). `cfg!` here is the HOST, as the Go
    // script's `runtime.GOOS` is.
    if cfg!(target_os = "macos") {
        println!("cargo:rerun-if-changed=resources/ai/.version");
    }
    println!("cargo:rerun-if-changed=../scripts/download-llama-server.go");

    generate_command_ids();

    // Ensure resources/ai/ is populated before tauri_build::build() validates the
    // resource glob in tauri.conf.json. The Go script is idempotent (skips when
    // .version matches) and symlinks from the main clone in worktrees instead of
    // re-downloading. Without this, a fresh worktree's first `cargo check` fails
    // with an opaque glob error.
    ensure_llama_resources();

    link_staged_safari_webkit();

    // `capabilities-e2e/playwright.json` is reachable only from a `playwright-e2e` build, because
    // `playwright:default` exists only when `tauri-plugin-playwright` is linked. Any other build
    // that globs it dies with "Permission playwright:default not found".
    //
    // ❌ Don't move it into `capabilities/` and have this script write it under the feature and
    // delete it without: `capabilities/` is shared by every cargo process in the worktree, so two
    // invocations with different features fight over the file. The rustdoc lane runs
    // `cargo doc --all-features` in its own target dir specifically so it runs BESIDE clippy and
    // the test lanes, which is exactly when that fight happens and `pnpm check rust` flakes.
    //
    // Nothing is written to the source tree at build time, so no ordering between concurrent
    // builds can produce a wrong capability set.
    #[cfg(not(feature = "playwright-e2e"))]
    tauri_build::build();

    #[cfg(feature = "playwright-e2e")]
    {
        // tauri-build emits the capabilities `rerun-if-changed` itself only on its default path;
        // with a custom pattern it's ours to declare, for both directories the glob covers.
        println!("cargo:rerun-if-changed=capabilities");
        println!("cargo:rerun-if-changed=capabilities-e2e");
        // `capabilities*` covers `capabilities/` AND `capabilities-e2e/`. A sibling directory
        // rather than a subdirectory because the `glob` crate has no brace expansion, so this is
        // how two directories get unioned in one pattern — and because it leaves the pattern every
        // non-feature build uses untouched.
        //
        // ⚠️ A custom pattern is invisible to PLUGIN build scripts, which glob the default
        // `capabilities/**/*` (`tauri-plugin/src/build/mod.rs`, passing `None`). That only matters
        // under `build.removeUnusedCommands`, which Cmdr doesn't set: turning it on would strip the
        // playwright plugin's commands from this build. See `capabilities/CLAUDE.md`.
        if let Err(error) =
            tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern("./capabilities*/**/*"))
        {
            panic!("tauri-build failed: {error:#}");
        }
    }
}

/// Compiles the frontend's authoritative command-id tuple into a Rust slice.
///
/// Breadcrumb command ids cross an untrusted IPC boundary, so Rust must validate
/// them without maintaining a second 100+ item registry. The deliberately narrow
/// parser fails the build if `command-ids.ts` stops being one single-quoted id per
/// line: a source-shape change must update this generator rather than silently
/// producing a partial privacy allowlist.
fn generate_command_ids() {
    const SOURCE_RELATIVE_TO_MANIFEST: &str = "../src/lib/commands/command-ids.ts";
    const START: &str = "export const COMMAND_IDS = [";
    const END: &str = "] as const";

    println!("cargo:rerun-if-changed={SOURCE_RELATIVE_TO_MANIFEST}");

    let manifest_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR for build scripts"),
    );
    let source_path = manifest_dir.join(SOURCE_RELATIVE_TO_MANIFEST);
    let source = std::fs::read_to_string(&source_path)
        .unwrap_or_else(|error| panic!("couldn't read {}: {error}", source_path.display()));
    let body = source
        .split_once(START)
        .and_then(|(_, after_start)| after_start.split_once(END).map(|(body, _)| body))
        .unwrap_or_else(|| panic!("{} must contain `{START}` followed by `{END}`", source_path.display()));

    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let id = trimmed
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix("',"))
            .unwrap_or_else(|| {
                panic!(
                    "{}:{} must be a single-quoted command id followed by a comma",
                    source_path.display(),
                    index + 1
                )
            });
        assert!(!id.is_empty(), "command ids must not be empty");
        assert!(
            id.chars().count() <= 128,
            "command id `{id}` exceeds the diagnostic boundary's 128-character cap"
        );
        assert!(
            id.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '.'),
            "command id `{id}` contains a character the generated Rust vocabulary does not permit"
        );
        assert!(seen.insert(id), "duplicate command id `{id}`");
        ids.push(id);
    }
    assert!(!ids.is_empty(), "the command-id registry must not be empty");

    let generated = format!(
        "&[\n{}]\n",
        ids.iter().map(|id| format!("    {id:?},\n")).collect::<String>()
    );
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts"));
    std::fs::write(out_dir.join("command_ids.rs"), generated).expect("the generated command-id vocabulary is writable");
}

/// Lets macOS 10.15 and 11 run Cmdr on the WebKit a Safari update installed, instead of the one
/// the OS shipped with.
///
/// A Safari update on those systems stages its WebKit under this directory and leaves
/// `/System/Library/Frameworks/WebKit.framework` alone, and dyld only picks the staged copy for a
/// process whose main executable carries this `LC_DYLD_ENVIRONMENT` command. Without it, a fully
/// updated Catalina hands Cmdr Safari 13.1's WebKit, which can't run the UI. dyld takes whichever
/// copy has the higher version, so a Mac without a newer staged copy loads the system one as before,
/// and one with a Safari update newer than its OS gets the WebKit Safari itself runs. Why and
/// evidence: `docs/notes/system-requirements-and-es2025.md` § "The WebKit an app gets on older
/// macOS". `desktop-macos-framework-floor` fails a build that loses it.
///
/// `-bins` because dyld reads the command only from the MAIN executable; tests and the lib don't
/// need it.
fn link_staged_safari_webkit() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    println!(
        "cargo:rustc-link-arg-bins=-Wl,-dyld_env,DYLD_VERSIONED_FRAMEWORK_PATH=/Library/Apple/System/Library/StagedFrameworks/Safari"
    );
}

fn ensure_llama_resources() {
    let status = std::process::Command::new("go")
        .args(["run", "scripts/download-llama-server.go"])
        .current_dir("..")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!(
            "download-llama-server.go failed with exit code {}. Run `cd apps/desktop && go run scripts/download-llama-server.go` to see the full output.",
            s.code().unwrap_or(-1),
        ),
        Err(e) => panic!(
            "Failed to invoke `go run scripts/download-llama-server.go`: {e}. Make sure `go` is on PATH (mise shims should handle this). This script downloads the llama-server binaries that Tauri bundles via `resources/ai/*`.",
        ),
    }
}
