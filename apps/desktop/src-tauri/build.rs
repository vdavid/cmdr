fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/ai/.version");
    println!("cargo:rerun-if-changed=../scripts/download-llama-server.go");
    // `analytics::posthog` reads this via `option_env!`, which is baked at compile time. Without
    // this line, changing the env var between builds wouldn't trigger a recompile of that consumer.
    println!("cargo:rerun-if-env-changed=CMDR_POSTHOG_KEY");

    // Ensure resources/ai/ is populated before tauri_build::build() validates the
    // resource glob in tauri.conf.json. The Go script is idempotent (skips when
    // .version matches) and symlinks from the main clone in worktrees instead of
    // re-downloading. Without this, a fresh worktree's first `cargo check` fails
    // with an opaque glob error.
    ensure_llama_resources();

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
