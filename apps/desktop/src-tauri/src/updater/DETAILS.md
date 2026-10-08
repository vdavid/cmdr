# Updater module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the must-knows that prevent silent breakage; this is the depth.

## Key decisions

- **Sync files into the bundle instead of replacing the `.app` directory.** ❌ Not because replacing would cost the FDA
  grant: it wouldn't. TCC's `access` table has no path or inode column, and the stored requirement for Cmdr is
  `identifier "com.veszelovszki.cmdr" and anchor apple generic and … certificate leaf[subject.OU] = "83H6YAQMNP"`, so a
  grant follows the signature, not the bundle on disk (verified on macOS 26.5.2, TCC.db inspection plus a
  launch/move/relaunch of a signed probe, 2026-08-25; `docs/notes/self-move-to-applications-2026-08-25.md`). What the
  decision actually rests on: `com.apple.macl` on the bundle is per-file and IS lost when the directory is recreated,
  and the per-file atomic rename below needs a bundle to sync into. Both hold. Only the FDA half of the old rationale
  was wrong, so don't reach for "it would cost FDA" when defending this.
- **Sync order: Resources, Info.plist, _CodeSignature, then the MacOS binary last.** Updating the binary last minimizes
  the window where the code signature is inconsistent with the binary on disk; if the app crashes mid-update, the old
  binary is still intact.
- **Unconditional deletion of stale files after sync.** Old files left behind could cause version mismatches or bloat.
  The deletion pass removes anything in the destination not in the source, then cleans empty directories bottom-up.
- **Minisign verification before writing the tarball to disk.** Ensures integrity and authenticity; the public key is
  compiled into the binary. Both key and signatures use base64(minisign-text-format), matching Tauri's convention.
- **Privilege escalation via `osascript` with `rsync -a --delete`.** When installed in `/Applications` (root-owned),
  direct writes fail; `osascript`'s `do shell script … with administrator privileges` shows the native auth dialog.
  `rsync` expresses the full sync (copy + delete stale) in one shell command. Only triggers when direct writes are
  denied, so users running from `~/Applications` or a dev build won't see the dialog. A `.pkg` install is always
  `root:wheel`, so it always takes this path; why that's accepted: `docs/guides/releasing.md` § The installer package.
- **Atomic rename instead of in-place `fs::copy`.** (Inode / code-signing-cache rationale is in `CLAUDE.md`.)
- **Per-instance staging dir: `<tmp>/cmdr-update-staging-{CMDR_INSTANCE_ID}`**, `…-default` for production with no env
  var set, so a main-clone and a worktree `Cmdr` never share one path.
- **Bounded manifest-fetch timeouts.** `reqwest::get`'s default client has no overall timeout; a stuck TCP handshake to
  the redirect target was observed hanging ~2.5 min, which made transient network blips look like a hung app and tripped
  the auto error reporter. Download/install stay untimed (user attention; can legitimately take a while).
- **Check the HTTP status before parsing the manifest.** A 5xx or an HTML maintenance page would otherwise deserialize
  into a parse failure, which reads as "the manifest is malformed" and sends the reader to the wrong layer: the manifest
  is fine, the server didn't serve it. `fetch_manifest` goes through `crate::server_request`, so a non-2xx comes back as
  `ServerRequestError::Refused { status }`, and only a 2xx that doesn't parse is `BadResponse`: the one failure the
  frontend logs at error, since it means Cmdr's server and this build disagree. The frontend owns the log line, once
  per condition (`apps/desktop/src/lib/updates/DETAILS.md`), so the command logs nothing of its own on a failure.
- **The tarball download is typed too (`UpdateDownloadError`).** `fetch_verified_tarball` also goes through
  `crate::server_request`, so a 5xx maintenance page is a `Request { Refused }` rather than bytes that fail their
  signature. `SignatureMismatch` and `Disk` stay separate variants: the frontend logs those at error and a network
  `Request` failure at warn. `install_update` answers `UpdateInstallError` (`BlockedByPolicy`, `NothingStaged`,
  `Failed { detail }`): every `Failed` is local and logs at error; `BlockedByPolicy` is quiet.
- **Walk `reqwest::Error::source()` for log-friendly messages (`crate::server_request::describe_error_chain`).** `reqwest::Error`'s `Display`
  only prints the outermost layer, hiding the real cause (DNS, TCP connect timeout, TLS). Walking the source chain
  surfaces the underlying class without pulling in `anyhow`.

## Who may check

`skip_reason` allows a check only from a real user's production install. Two conditions: the exe must sit inside a
`.app` bundle (`installer::is_running_from_app_bundle`), and none of `crate::prod_instance::NON_PROD_ENV_VARS` may be
set. Outside a bundle the updater can't work and would spam noisy errors into the auto error reporter; a tooling
instance that slips through writes an `update_checks` row the dashboard counts as an active install. Don't loosen
either. `crate::prod_instance` is the one definition of the env-var list, shared with the analytics gate so the two
can't disagree about what a real install is.

The manifest URL (`https://api.getcmdr.com/update-check/{version}?arch={arch}`) is built at runtime from the
compile-time version and arch; the API server logs the check to D1 for active-user counting, then 302-redirects to
`https://getcmdr.com/latest.json`.

## Managed policy (MDM)

The organization's `DisableUpdates`, `DisableAutomaticUpdateChecks`, and `MaxUpdateVersion` (key catalog:
`managed_policy/DETAILS.md`) apply at every step, each on a fresh read (`managed_policy::for_egress`):

- **Check.** `check_for_update(trigger)` answers `UpdateCheckOutcome`: `UpToDate`, `Available { version }`,
  `HeldByPolicy { available, ceiling }`, `UpdatesDisabledByPolicy`, or `AutomaticChecksDisabledByPolicy`. The last two
  return before any request (so no `update-check` row either). `trigger` is the Rust `UpdateCheckTrigger` enum,
  passed by the frontend and carried by its `update_check` analytics event too; `is_automatic` decides: `startup` /
  `poll` / `auto_check_on` are automatic, `command` / `settings` are a person asking, which
  `DisableAutomaticUpdateChecks` still allows. The policy is asked BEFORE `skip_reason`, so a dev build run with
  `CMDR_MANAGED_PREFS_FILE` shows the managed answer. The backend trusts that trigger; why that's accepted:
  `managed_policy/DETAILS.md` § Accepted residuals.
- **Offer.** Only `Available` stores the release (`UpdateInfo`: version, URL, signature) in `UpdateState.offered`; every
  check clears the slot first, so a download can only fetch what the newest check offered under the newest policy.
- **Download.** `download_update` takes no arguments: it fetches the offered URL, never one the frontend names, after
  `ManagedPolicy::update_to(version)` on a fresh read (`BlockedByPolicy`, `NothingOffered`). The staged slot records the
  version beside the tarball path.
- **Install.** `install_update` re-asks `update_to` for the staged version on a fresh read, so a download staged before
  a profile arrived doesn't install. Then `installer::vet_staged_bundle` asks again for the version the extracted
  `Info.plist` names: the manifest isn't signed, so it could offer 0.52.1 under a `"0.52"` ceiling and serve a genuine,
  signed 0.53.0. The rollback check (`refuse_unless_newer`) runs first, so an older archive is a `Failed`, not a policy
  refusal.
- **Already synced.** A build `install_update` already wrote into the bundle (frontend `ready`) applies at the next
  restart whatever the policy says later: the sync is the install.
- The ceiling compares the release core only (`UpdateCeiling::allows`), so `0.53.0-rc.1` doesn't pass `"0.52"`.
- Out of scope: the Linux Tauri-plugin path (the frontend calls `@tauri-apps/plugin-updater` directly), since the policy
  source is macOS-only.

## A bundle that can't be written

Two macOS arrangements make the running `.app` unwritable, and the app can't fix either from inside:

- **App Translocation.** Opening Cmdr straight from where it was downloaded makes Gatekeeper run it from a randomized
  read-only mount under `/private/var/folders/…/AppTranslocation/`, rather than from its real path.
- **A mounted disk image.** The `.dmg` is still open and the app was launched from inside it.

Both fail writes with `EROFS`, which is `io::ErrorKind::ReadOnlyFilesystem`, NOT `PermissionDenied`. That matters twice
over: the escalate-to-admin arm never fired for them (so the install hard-failed with nothing said), and escalating
would not have helped anyway, since a read-only mount refuses root too. The signature is an install that keeps sending
update checks and never changes version, which is exactly the straggler shape the update dashboard couldn't explain.

`bundle_location::classify` answers with a `BundleWriteBlocker`, using two probes:

- `SecTranslocateIsTranslocatedURL` (Security.framework, macOS 10.12+) for the translocation case. The alternative,
  testing the path for `/AppTranslocation/`, is a private layout Apple never promised and whose break we'd never notice.
- `statfs`'s `MNT_RDONLY` as the catch-all, which covers a disk image, a read-only share, and translocation itself.

Translocation is reported in preference to the read-only volume it implies, so the log names the outer cause. Every
failure path answers "no blocker": a false negative costs a doomed download, while a false positive would stop updates
that would have worked.

Two callers gate on it. The frontend asks (`update_write_blocker`) once a check finds an update and before the
download, which is what stops an install pulling ~63 MB it can't apply once per poll interval forever, and raises the
"move Cmdr to Applications" dialog instead. `installer::install` asks again before extracting, so a direct caller can't
skip the gate; and `sync_bundle`'s `ReadOnlyFilesystem` arm answers the nested case without raising an admin prompt.

## Dependencies

- `reqwest`: HTTP client for manifest + tarball download.
- `minisign-verify`: signature verification.
- `flate2`, `tar`: tarball extraction.
- `filetime`: touches the bundle after install to trigger a LaunchServices refresh.
- `base64`: decodes the double-encoded minisign key and signatures.
