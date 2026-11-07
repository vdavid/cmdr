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
- **Minisign verification before writing the tarball to disk.** Ensures integrity and authenticity; the public keys
  (main and backup, § Two updater keys) are compiled into the binary. Both keys and signatures use base64(minisign-text-format), matching Tauri's convention.
- **No privilege escalation: a bundle this user can't write gets a notice, never an admin prompt.** An admin-run helper
  can't write a notarized app on macOS 13+: App Management (`kTCCServiceSystemPolicyAppBundles`) lets only code signed
  by the app's own team (or a process the user granted App Management or Full Disk Access) modify it, and
  `do shell script … with administrator privileges` runs through Apple's `authtrampoline`, which TCC holds responsible
  and refuses without even asking. Root doesn't help. The `osascript` + `rsync` escalation this replaces prompted for a
  password on every background check and then failed `EPERM` (verified on macOS 27.0.1, TCC log on the M1,
  2026-10-20; `docs/notes/2026-10-21-admin-update-app-management.md`). Cmdr writing its OWN bundle is same-team, which
  is why every user-owned install updates. A `.pkg` install is `root:wheel`, so it never self-updates: IT pushes each
  new package (`docs/guides/releasing.md` § The installer package). Making root-owned installs update would take a
  privileged helper signed by Cmdr's team (what Sparkle 2 does); not built, since `.pkg` installs are MDM-managed.
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
  `Request` failure at warn. `install_update` answers `UpdateInstalled { securityNotice }` (§ Security notices) or
  `UpdateInstallError` (`BlockedByPolicy`, `NothingStaged`, `Failed { detail }`): every `Failed` is local and logs at
  error; `BlockedByPolicy` is quiet.
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

**The archive URL is whatever the manifest names, on any HTTPS host.** No host allowlist, here or in any shipped
version: trust rests on the minisign signature and the Developer ID check, so the manifest can move downloads (to
`download.getcmdr.com`, the EU mirror, since #805) without an app change. ❌ Don't add a host check: every installed copy
would then be stuck on that host for good. `docs/guides/releasing.md` § Where downloads are served from.

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
  signed 0.53.0. The code-signature check runs first (§ Code signature), then the rollback check
  (`refuse_unless_newer`), so an older archive is a `Failed`, not a policy refusal.
- **A version that isn't genuine.** A tarball that fails minisign (`SignatureMismatch`) or a bundle refused for its
  code signature (`UntrustedCodeSignature`) puts its version in `UpdateSlots::not_genuine` for the session, and
  `offer_to_download` refuses that version with `NotGenuineEarlier` before fetching, so the background loop can't
  re-download a fake every run. A newer version still goes through, so a real fix can arrive (David, 2026-10-16). Both
  refusals reach the security log once (`security_events.rs`), which raises the person's notice
  (`../security_notices/CLAUDE.md`); the frontend's standing status words all three the same (`notGenuine`).
- **Already synced.** A build `install_update` already wrote into the bundle (frontend `ready`) applies at the next
  restart whatever the policy says later: the sync is the install.
- The ceiling compares the release core only (`UpdateCeiling::allows`), so `0.53.0-rc.1` doesn't pass `"0.52"`.
- Out of scope: the Linux Tauri-plugin path (the frontend calls `@tauri-apps/plugin-updater` directly), since the policy
  source is macOS-only.
- **No install, no notice.** Under `DisableUpdates` or a held version nothing is staged, so no bundle can say a fix
  exists. Advisories for managed fleets go out through the channels `docs/security-incident-procedure.md` lists.

## Code signature

Before anything is written into the running bundle, `installer::vet_staged_bundle` asks
`cmdr_update_trust::verify_release_bundle` whether the extracted `Cmdr.app` carries a valid Apple code signature from
Cmdr's Developer ID, so a leaked updater key alone can only produce archives every install refuses. The check itself
(the API, the requirement, the extraction it depends on, why there's no switch, and its verification record) lives in
`crates/cmdr-update-trust/DETAILS.md`, shared with the release gate that refuses to sign or publish an archive this
check would refuse. What's the updater's own:

- **Order.** The check runs first in `vet_staged_bundle`, before the `Info.plist` version is believed, and refuses with
  `UpdateInstallError::UntrustedCodeSignature { detail }`. Nothing is synced, the staging dir is left for the next
  attempt to clear, and the running app is untouched. The backend logs it at error; so does the frontend.
- **Nobody can skip it.** Dev builds never get this far anyway: `skip_reason` stops them at the check. Unit tests inject
  the check through `vet_staged_bundle`'s `code_signature` parameter.
- The install is untimed and runs on a blocking thread, so the check's under-a-second cost is invisible.

**The whole path proven end to end** (a pipeline-built archive, the release gate, and a real installed app updating
itself and refusing doctored archives): `docs/notes/2026-10-15-update-path-proof.md`.

## Security notices

When an update fixes a vulnerability, the restart toast says so and what to do (the CRA's advisory-message point). The
list itself, what gets an entry, and the link allowlist: `../security_advisories/DETAILS.md`. What's the updater's own:

- **Read from the staged bundle, never from `latest.json`.** The manifest isn't signed (the minisign trusted comment
  names only `file:Cmdr.app.tar.gz`), so a `security` field there would let anyone who can serve or tamper with it
  show a fake "critical" notice with a phishing link, or hide a real one. The staged bundle's
  `Contents/Resources/security-advisories.json` sits inside the minisign-verified archive AND under the Developer ID
  code signature, which seals `Contents/Resources/`. ❌ Nothing in `latest.json` may show, hide, or change a notice;
  a mirror of the flag there, if ever added for the website, is for the website only.
- **Order.** `vet_staged_bundle` reads the list (its injected `read_advisories`) only after the code signature, the
  rollback check, and the policy have all passed. Before that the file is just bytes the updater key's holder chose.
  `the_advisory_list_of_an_untrusted_or_refused_bundle_is_never_read` pins it.
- **Never in the way.** A missing, unreadable, or malformed list is logged at warn and reads as no notice; the install
  goes on. The update is the security fix itself.
- **The window is (running, staged].** `advisories::notice_from` keeps `running < fixedIn <= staged` from the STAGED
  list, which is cumulative, so a jump over several releases counts every fix in between. The notice carries the
  highest severity, the count, and the most severe advisory's vetted link (`None` when the allowlist refuses it).
- **Superseded staging replaces it.** A newer staged build answers its own notice, and the frontend replaces the old
  one rather than merging (`apps/desktop/src/lib/updates/DETAILS.md` § The security variant).
- Out of scope: the Linux plugin path, which reads no list (it follows the unsigned `notes`), until Linux leaves alpha.

## Two updater keys

`signature::verify` accepts a signature from either key in `TRUSTED_PUBKEYS`, and refuses everything else:

- **The main key** (`A2601F36BB168C0A`) signs every release. `MAIN_PUBKEY_BASE64` is a copy of
  `plugins.updater.pubkey` in `tauri.conf.json`, which has to stay there because the Tauri bundler reads it; the test
  `the_main_key_is_the_one_in_tauri_conf` pins the two equal.
- **The offline backup key** (`B72B6B32B7D71B58`) exists so a leaked or lost main key can be rotated out with one
  release signed by the backup. Its private half lives only in David's Bitwarden. Its public half's one home is
  `apps/desktop/src-tauri/updater-backup.pub`, which the app compiles in (`include_str!`) and
  `scripts/release-finish` reads, so the two verifiers can't disagree. A separate file because `plugins.updater` in
  `tauri.conf.json` takes one key, and that section belongs to the Tauri plugin's config, not ours.
- A signature names its key ID, so minisign-verify refuses a mismatched key before hashing anything: trying each key
  costs one real check.
- **Every verifier must trust the same set**, or a backup-signed release is refused before it ships:
  `release-finish.sh` (the laptop's check after signing, and `-verify-dir`, the `publish` job's gate) reads both through
  `readTrustedPubkeys`. The Tauri updater plugin (non-macOS builds) trusts only the `tauri.conf.json` key, and
  `latest.json` publishes no Linux build today.
- The rotation runbook (what the rotation release changes, how it signs without the backup key entering sops):
  `docs/guides/releasing.md` § Rotating to the backup updater key.

Fixture: `testdata/fixture.pub` and `payload.txt.sig` come from a throwaway key (`tauri signer generate`, then
`tauri signer sign`), so the tests pin the verifier to the real signer's output.

## A bundle that can't be written

Three arrangements make the running `.app` unwritable, and the app can't fix any of them from inside:

- **App Translocation.** Opening Cmdr straight from where it was downloaded makes Gatekeeper run it from a randomized
  read-only mount under `/private/var/folders/…/AppTranslocation/`, rather than from its real path.
- **A mounted disk image.** The `.dmg` is still open and the app was launched from inside it.
- **A bundle this user can't write.** In practice a `.pkg` install (`root:wheel`), as MDM tools deploy it.

The first two fail writes with `EROFS` (`io::ErrorKind::ReadOnlyFilesystem`), and a read-only mount refuses root too.
The third fails `EACCES`, and admin rights don't fix it either (§ Key decisions, no privilege escalation). All three
share a signature: an install that keeps sending update checks and never changes version, the straggler shape the
update dashboard couldn't explain.

`bundle_location::classify` answers with a `BundleWriteBlocker`, using three probes in this order:

- `SecTranslocateIsTranslocatedURL` (Security.framework, macOS 10.12+) for the translocation case. The alternative,
  testing the path for `/AppTranslocation/`, is a private layout Apple never promised and whose break we'd never notice.
- `statfs`'s `MNT_RDONLY` as the catch-all, which covers a disk image, a read-only share, and translocation itself.
- `access(W_OK)` on every folder under `Contents/`. Folders only: the sync replaces a file with a temp file plus
  `rename()`, which needs the folder writable, never the file. Symlinks aren't followed (the sync replaces a link, it
  doesn't write through one), and the bundle folder itself is skipped (the install only touches its mtime, and a
  failure there is a warn). It blocks only on a definite `EACCES`. The answer splits on `mbr_check_membership` against
  the `admin` group: `NotWritableAdminUser` (Finder can replace the app after their password) or
  `NotWritableStandardUser` (only whoever manages the Mac can). An unknown membership reads as standard, whose copy is
  true for anyone. Verified on a real root-owned `/Applications/Cmdr.app` (blocked at `Contents`) and a user-owned copy
  of the same bundle (no blocker), macOS 27.0.1, 2026-10-21.

Translocation is reported in preference to the read-only volume it implies, so the log names the outer cause, and both
before permissions, since a read-only mount refuses whoever owns the bundle. Every failure path answers "no blocker": a
false negative costs a doomed download, while a false positive would stop updates that would have worked.

Two callers gate on it. The frontend asks (`update_write_blocker`, under a 5 s `blocking_with_timeout` that also answers
"no blocker") once a check finds an update and before the download, which is what stops an install pulling ~63 MB it
can't apply once per poll interval forever, and raises the right notice instead (`apps/desktop/src/lib/updates/DETAILS.md`
§ When the bundle can't be written). `installer::install` asks again before extracting, so a direct caller can't skip the
gate, and answers `UpdateInstallError::Unwritable { blocker }`. `installer::sync_failure` maps a refusal the probe missed
(a nested mount, a file flag) onto the same variant: `ReadOnlyFilesystem` to `ReadOnlyVolume`, `PermissionDenied` to the
permission blocker. Neither is a failure, so neither files an error report.

## Local Network after an update

macOS's Local Network rule is per bundle id, but the kernel matches processes to it by main-executable UUID, which
`nehelper` copies from LaunchServices on an "apps installed" notification. The sync's FSEvents and `touch_bundle` fire
that notification, yet `nehelper` reads the user's LaunchServices record before it catches up with the new binary, so
the rule keeps the replaced binary's UUID. The relaunched app then falls outside its own rule: blocked until someone
switches Cmdr off and on in System Settings (#1019; verified on macOS 27.0.1 with real 0.58.0 → 0.59.0 in-app updates,
three of three, 2026-11-06; `docs/notes/local-network-after-update-2026-11-06.md`).

- **The fix runs on the first launch after an update** (`local_network_refresh::spawn_after_update`, gated by
  `crate::after_update`'s version stamp): `LSRegisterURL(bundle, true)` makes the user record current, then
  `touch_bundle` makes `nehelper` re-read it. Measured: the new UUID lands in the rule about 1.5 s later, long before the
  launch sweep's first dial (about 15 s in).
- **Why at launch, not right after the sync.** The old process is still running when the sync finishes, and it needs
  the OLD UUID in the rule until the person restarts. It also means the first release carrying this repairs the very
  update that installed it.
- **Why both calls.** `LSRegisterURL` alone doesn't notify `nehelper` (measured). The touch alone works when the user
  record is already current, which at launch it usually is; the re-register makes that deterministic.
- Only an installed production copy does it (`skip_reason` none, in an Applications folder), on its own 8 MB-stack
  thread, since LaunchServices answers over XPC.
- The backstop when it doesn't help: the share notice speaks up about a dial this Mac blocked on that launch
  (`../network/DETAILS.md` § This Mac refusing the route).

## The restart, and a second Dock icon

The update toast's Restart calls Cmdr's own `relaunch_app`: quit (through the quit gate), then a detached waiter that
outlives us waits for this process to exit and runs `/usr/bin/open <bundle> --args …` without `-n`. One instance at a
time, so LaunchServices and the Dock bind the new process to the pinned tile. Mechanism and decisions:
`../quit/DETAILS.md` § Relaunch.

Why not Tauri's `relaunch()`: Tauri 2.12.2+ restarts with `/usr/bin/open -n <bundle>` and then exits. `-n` starts a
second instance while the old one still runs, the Dock gives that instance its own tile, and once the old process (the
one bound to the pinned tile) exits, the pinned tile stays dark while an extra Cmdr tile carries the running dot until
the next quit (#1037). The sync itself doesn't do this: an update from 0.51.0 or older (Tauri 2.12.1, which spawned the
binary directly) left the pinned tile lit (verified on macOS 27.0.1, real 0.50.0/0.51.0/0.55.0/0.59.0 → 0.60.0 updates
through the toast, 2026-11-07; `docs/notes/dock-duplicate-icon-2026-11-07.md`).

The restart that runs is the OLD version's code, so the fix shows from the update AFTER the first build carrying it.
Anyone already showing two tiles needs nothing: the first relaunch without `-n`, or any quit and reopen, lights the
pinned tile again.

## Dependencies

- `reqwest`: HTTP client for manifest + tarball download.
- `minisign-verify`: signature verification.
- `cmdr-update-trust`: tarball extraction, the bundle's version, and the Apple code-signature check.
- `filetime`: touches the bundle after install to trigger a LaunchServices refresh.
- `base64`: decodes the double-encoded minisign key and signatures.
