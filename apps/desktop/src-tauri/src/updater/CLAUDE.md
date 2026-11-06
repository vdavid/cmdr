# Updater module

Custom macOS updater that syncs files *into* the existing `.app` bundle, keeping its `com.apple.macl` xattr and giving
each per-file write the fresh inode the code-signing cache needs. macOS-only; other platforms use the Tauri updater
plugin from the frontend.

## File map

- `mod.rs`: the four Tauri commands, shared `UpdateState`, and the typed outcomes and errors. Tests in `tests.rs`.
- `bundle_location.rs`: whether the running bundle can be written into at all (`BundleWriteBlocker`).
- `manifest.rs`: parses `latest.json`, compares versions, resolves the platform key.
- `signature.rs`: minisign verification against the main and backup updater keys.
- `security_events.rs`: the security event log's updater events (`../security_log/CLAUDE.md`).
- Code-signature check and tarball extraction: `crates/cmdr-update-trust/CLAUDE.md` (shared with the release gate).
- `installer.rs`: extraction, the sync into the running bundle, and what a refused sync answers. `running_bundle()` is
  crate-visible (`crate::install_location`).
- `advisories.rs`: the `SecurityNotice` an install answers, from the staged bundle's list.
- `local_network_refresh.rs`: the first launch after an update re-announces the bundle (#1019).

## Must-knows

- **Sync into the bundle, never replace the `.app` directory**, because the per-file atomic rename below needs a bundle
  to sync into. ❌ NOT because replacing loses the FDA grant: it doesn't (measured; `DETAILS.md`).
- **Per-file writes use temp + `rename()`, never in-place `fs::copy`.** The code-signing cache keys on inode, so a
  same-inode copy dies with `SIGKILL (Code Signature Invalid)`.
- **Staging dir is per-instance** (`CMDR_INSTANCE_ID`): a shared one makes a main and a worktree `Cmdr` trip
  `ENOTEMPTY`.
- **Only a real user's production install may check** (`skip_reason`). ❌ Never keep a second copy of
  `crate::prod_instance::NON_PROD_ENV_VARS` here. `DETAILS.md` § Who may check.
- **❌ Never escalate to admin**: App Management (macOS 13+) refuses an admin-run helper anyway. An unwritable bundle
  answers `bundle_location::classify` BEFORE the download, a refused sync `UpdateInstallError::Unwritable`. The probe
  leans "writable": a false block stops working updates. `DETAILS.md` § A bundle that can't be written.
- **Minisign (main or offline backup key) on the archive, AND Cmdr's Developer ID on the bundle.**
  `vet_staged_bundle` refuses a bundle not code-signed by team `83H6YAQMNP` (`UntrustedCodeSignature`) before reading
  it; ❌ nothing may skip that, tests inject it. The backup pubkey lives only in `updater-backup.pub`, also read by
  `scripts/release-finish`; ❌ never copy it. `DETAILS.md` § Code signature, § Two updater keys.
- **The signature doesn't name a version, so the archive's own `Info.plist` does.** Neither the trusted comment nor
  `latest.json` names one, so `installer::refuse_unless_newer` refuses a staged bundle that isn't newer than the
  running build. ❌ Never install around it: it's what stops an older signed release rolling an install back.
- **The organization's policy gates every step, and the backend decides.** `check_for_update(trigger)` answers a typed
  `UpdateCheckOutcome` (`DisableUpdates` and a refused background check return before any request); `download_update`
  takes no URL and fetches only what the last check offered; `install_update` re-asks
  `ManagedPolicy::update_to` on a fresh read, then again for the version the extracted `Info.plist` names. ❌ Never let
  the frontend name a URL or version. `DETAILS.md` § Managed policy.
- **Security notices come only from the vetted staged bundle, ❌ never `latest.json`** (unsigned). Read after every
  vet check passes; a broken list ❌ never blocks the install. `DETAILS.md` § Security notices.
- **An update leaves Local Network on the old binary's UUID**; the next launch re-announces it (❌ not in `install`, ❌
  not without the touch). `DETAILS.md` § Local Network after an update.
- **Manifest fetch is bounded** (10 s connect, 30 s overall); download and install are untimed. The check and the
  download ride `server_request::send` (`Egress::UpdateCheck`, `UpdateDownload`).

Sync order, signatures, why no escalation, who may check, the policy, and Local Network: `DETAILS.md`. Read it before
any non-trivial work here.
