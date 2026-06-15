# Updater module

Custom macOS updater that syncs files *into* the existing `.app` bundle, preserving its inode and `com.apple.macl` xattr
so macOS TCC (Full Disk Access) permissions survive updates. macOS-only (`#[cfg(target_os = "macos")]`); other platforms
use the Tauri updater plugin and the frontend calls the plugin API directly.

## File map

- `mod.rs`: the three Tauri commands (`check_for_update`, `download_update`, `install_update`) and shared `UpdateState`.
- `manifest.rs`: parses `latest.json`, compares versions, resolves the platform key.
- `signature.rs`: minisign signature verification (base64-wrapped, matching Tauri's format).
- `installer.rs`: tarball extraction, sync into the running bundle, privilege escalation.

## Must-knows

- **Sync into the bundle, never replace the `.app` directory.** Replacing it changes the inode and macOS TCC loses FDA
  grants, forcing the user to re-grant after every update. The install path fundamentally can't work outside a bundle
  (no `Contents/` to sync into).
- **Per-file writes use atomic rename (temp + `rename()`), not in-place `fs::copy`.** `fs::copy` keeps the same inode;
  macOS's kernel code-signing cache keys on inode and validates the new binary against the old cached code directory,
  causing `SIGKILL (Code Signature Invalid)` on launch. A new inode forces fresh validation. The admin path (`rsync -a`)
  already renames atomically.
- **Staging dir is per-instance: `<tmp>/cmdr-update-staging-{CMDR_INSTANCE_ID}`** (`installer::staging_dir`; production
  with no env var lands at `…-default`). Don't make it shared: concurrent `Cmdr` processes (main + a worktree) race on
  one path and trip `ENOTEMPTY`.
- **Dev-build guard:** `check_for_update` returns `None` when the exe isn't inside a `.app` bundle
  (`installer::is_running_from_app_bundle`). Don't loosen it: outside a bundle the updater can't work and would spam
  noisy errors into the auto error reporter.
- **CI guard:** `check_for_update` returns `None` when `CI` is set, so no network calls in tests.
- **Manifest fetch is bounded** (`connect_timeout` 10 s, overall `timeout` 30 s); download/install paths are
  intentionally NOT timed out (they run with user attention). Don't add timeouts there.
- **Manifest URL routes through the API server** (`https://api.getcmdr.com/update-check/{version}?arch={arch}`), which
  logs the check to D1 for active-user counting, then 302-redirects to `https://getcmdr.com/latest.json`. Built at
  runtime from the compile-time version and arch.

Full details (sync order, deletion pass, minisign rationale, privilege escalation, error-chain logging,
dependencies): [DETAILS.md](DETAILS.md).
