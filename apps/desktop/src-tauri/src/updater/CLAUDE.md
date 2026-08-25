# Updater module

Custom macOS updater that syncs files *into* the existing `.app` bundle, preserving its inode and `com.apple.macl` xattr
so macOS TCC (Full Disk Access) permissions survive updates. macOS-only (`#[cfg(target_os = "macos")]`); other platforms
use the Tauri updater plugin and the frontend calls the plugin API directly.

## File map

- `mod.rs`: the four Tauri commands (`check_for_update`, `update_write_blocker`, `download_update`, `install_update`)
  and shared `UpdateState`.
- `bundle_location.rs`: whether the running bundle can be written into at all (`BundleWriteBlocker`).
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
- **Only a real user's production install may check** (`skip_reason`). Two conditions: the exe must sit inside a `.app`
  bundle (`installer::is_running_from_app_bundle`), and none of `crate::prod_instance::NON_PROD_ENV_VARS` may be set.
  Outside a bundle the updater can't work and would spam noisy errors into the auto error reporter; a tooling instance
  that slips through writes an `update_checks` row the dashboard counts as an active install. Don't loosen either, and
  ❌ never keep a second copy of the env-var list here: `crate::prod_instance` is the one definition, shared with the
  analytics gate so the two can't disagree about what a real install is.
- **A read-only bundle is EROFS, not EPERM, and no amount of admin fixes it.** App Translocation (Cmdr opened from
  `~/Downloads`) and a mounted `.dmg` both put the bundle on a read-only mount, which refuses root as flatly as the
  user. `installer::install` and the frontend both gate on `bundle_location::classify` BEFORE the download, ❌ never by
  escalating: escalating buys an auth dialog the user can only cancel. The `PermissionDenied` arm is for a root-owned
  `/Applications`, a different thing. `DETAILS.md` § A bundle that can't be written.
- **Manifest fetch is bounded** (`connect_timeout` 10 s, overall `timeout` 30 s); download/install paths are
  intentionally NOT timed out (they run with user attention). Don't add timeouts there.
- **Manifest URL routes through the API server** (`https://api.getcmdr.com/update-check/{version}?arch={arch}`), which
  logs the check to D1 for active-user counting, then 302-redirects to `https://getcmdr.com/latest.json`. Built at
  runtime from the compile-time version and arch.

Full details (sync order, deletion pass, minisign rationale, privilege escalation, error-chain logging,
dependencies): `DETAILS.md`.
