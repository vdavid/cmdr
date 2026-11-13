# About this (host) details

The app side of "Tell me about this" (#1104). The vocabulary, the plan, and every rule live in
`crates/cmdr-about/DETAILS.md`; this file covers how the host gathers and streams.

## The stream

`about_start(request, onEvent)` returns at once and spawns the gathering. Every message is
`AboutEvent { requestId, update }`, and `update` is one of:

- `planned { plan, derived }`: first, before any job is submitted. The plan is `cmdr_about::card::plan` trimmed by
  `jobs::GATHERED`, so the card reserves every row from frame one.
- `fact { fact, derived }`: one per row, in any order. The probe's identity lands right after `planned`; the jobs that
  answer inside `session::GRACE` (100 ms, the context menu's) land next; later ones land one by one.
- `factsSettled`: once, when every planned row has its answer. Nothing about facts follows.
- `unavailable { why }`: instead of all of the above, when the path can't be placed or described
  (`AboutUnavailable`).

**Decision: `derived` rides every `planned` and `fact` event.** `headline` and `consequences` are free functions over
the card, and both change as facts land (a size arrives, a download host arrives, the removal row moves from `CantTell`
to `UserDocument`). Sending them beside each fact keeps them current atomically with the fact that changed them, and the
frontend never runs a rule. A separate `derived` event would let the card render one state with the other's headline
for a frame. They're a few hundred bytes.

**Decision: the frontend mints the request id.** A card closed before `about_start` resolves can still cancel. The
registry treats a cancel for an id it hasn't seen as a tombstone (60 s, at most 256), and the start that follows
returns without gathering. A second start under a live id cancels the first. `REGISTRY.finish` removes only its own run.

**Decision: `about_start` returns nothing, and the request carries no target hint.** The id is the frontend's, so
returning it would only echo it. Whether the item is a file, folder, app, or link comes from the probe's `lstat` (or
the volume's metadata), never from what the pane believed.

## Wiring the card's frontend (first steps for whoever builds it)

`about_start` is registered (dispatch-only, raw invoke, beside `stream_folder_suggestions`). `about_cancel` is built and
tested but NOT registered: `desktop-ipc-unused` refuses a typed command with no caller, and its allowlist needs
David's consent. So the card's first commit, in one go:

1. Add `crate::about::commands::about_cancel` to the `cfg(all())` `typed` list in `ipc.rs`'s `ipc_command_manifest!`,
   and drop its `#[cfg_attr(not(test), expect(dead_code, …))]` in `commands.rs`.
2. `pnpm bindings:regen` (from `apps/desktop`): `commands.aboutCancel` appears beside the already exported
   `AboutEvent`.
3. A new `about.ts` in `apps/desktop/src/lib/tauri-commands/`, exported from the barrel: a start that mints the request id
   (`crypto.randomUUID()`), opens a `Channel<AboutEvent>`, and raw-invokes `about_start` with `{ request: { requestId,
   path, volumeId }, onEvent }` (the `cmdr/no-raw-tauri-invoke` opt-out, as `streamFolderSuggestions` in `settings.ts`
   does), and a cancel through `commands.aboutCancel`. The card's state slice is its caller, so the check passes.

## Routing

`route::gather_for` resolves through `crate::location::resolve` (a miss is `unavailable`), then `route::reach`:

- **`Local`** (a live `VolumeKind::Local`): `probe` on the about pool under `PROBE_BUDGET` (1.5 s; past it,
  `Unreachable`). The probe is one `lstat`, a mount-table read, for a dotted folder one `NSURLIsPackageKey` lookup,
  and on a local disk the look for a repo around the item (`git::repo_around`, through the app's repo cache), so the
  plan knows whether it has a git row.
  - The route also asks `OwnerQuestion::of(path, home)` (pure, no IO), so the plan knows whether it has an owner row,
    and places the item in the index (`index::IndexPlace`: the index id, which isn't always the volume's, and the
    importance store's path for it).
  - The mount table must call the item's mount a local disk (`mount_is_local_disk`). A path whose parent chain reaches a
    share through a symlink resolves to the boot volume and isn't local: it answers identity only.
  - A path inside an archive (`lstat` fails, `confirm_archive_boundary` holds) is an `ArchiveEntry` with identity
    `Unknown(VolumeCantTell)` until the archive arm reads its listing.
  - `SF_DATALESS` (an evicted cloud file) sets `contents_local: false`, so the plan has no real-type or signature row.
- **`ThroughTheVolume`** (every other kind, and a local volume that isn't registered): `Volume::get_metadata` under
  `REMOTE_BUDGET` (2 s). `VolumeCaps::identity_only` gives the card its volume note. A symlink reads as what the
  volume resolved it to, since no remote metadata carries a link target.

## Jobs and budgets

`jobs::GATHERED` is what this host answers: identity (from the probe), real type, bundle, signature, download, Cmdr's
own history, last opened, default app, importance, the owner, git, and a folder's whole size. A folder's digest and an
app's Library data are trimmed from the plan until their jobs exist, and so is any row whose store isn't there
(`jobs::Sources`: no operation log, no index place). Each row has `ROW_BUDGET` (1 s)
from the start of the gathering; a row still out then lands `Unknown(TimedOut)` and its job's late answer is dropped.
The `about_facts` pool (4 target, 12 max workers) bounds how many threads a dead mount can strand.

- **Real type**: the first 64 KB and, past that, the last 512 bytes (`TAIL_BYTES`: a DMG is told by its `koly`
  trailer), into `cmdr_about::sniff::real_type`.
- **Bundle**: the top-level `Contents/Info.plist` (or a root `Info.plist`), refused past 1 MB before parsing.
- **Download**: macOS `kMDItemWhereFroms` + `com.apple.quarantine`, Linux `user.xdg.origin.url` +
  `user.xdg.referrer.url`. `platform/unix.rs::read_capped` asks the size first and never reads past 64 KB; one past the
  cap goes to the decoder as an oversized buffer, which it reads as unparseable.
- **Last opened**: `MDItemCopyAttribute` for `kMDItemLastUsedDate` and `kMDItemUseCount`, through
  `cmdr_about::payload::usage_fact`, which reads a date sitting on the last save as `NoRecord` (Spotlight stamps a file
  an app writes; evidence in the crate's DETAILS).
- **Default app**: `URLForApplicationToOpenURL:`, named through `open_with::read_app_display_name`.
- **Cmdr's own history**: one read of `operation-log.db` (§ Cmdr's own history).

## Cmdr's own history

`history.rs`. The row says which Cmdr operation put the item where it is ("Copied from Naspolya on …"), and only when
the log recorded THIS node landing there.

- **The node comes from the probe's `lstat`** (`Probe::node`, inode + birth time), and `cmdr_operation_log::query::landed_at`
  matches it against the row's recorded `dest_inode` + `dest_birth_ns`. A file saved over since, by a temp and a
  rename as most apps save, is a new node: `Unknown(NodeChangedSince)` (Notable), ❌ never the operation. No row, a row
  with no node (written before nodes were kept, a remote destination, FAT), or a pruned op is `Unknown(NoRecord)`
  (Routine). The rule lives beside the schema (`crates/cmdr-operation-log/DETAILS.md` § "Where an item landed"); this
  module only words the answer.
- **The database only, never the disk**: its own read connection on the about pool, through
  `operation_log_host::db_path()`. No log open (a failed start) leaves the row out of the plan, since a row with no job
  would never settle; no file yet is `NoRecord`; a file that isn't a journal is `Unparseable`, logged by kind only.
- **Asked under two volume ids**: the local engine journals every OS-visible path under the local disk's id, whichever
  volume the pane showed it on, and the volume engine under the destination volume's own (an extract onto an external
  disk). The newest operation across both wins, then `NodeChangedSince`, then `NoRecord`.
- **Wording**: an extract is journaled as a copy whose source path runs through an archive, so a copy from a path with
  an archive component and something after it reads `Extract`. An agent plan the person edited is still Ask Cmdr's.
  The source volume is named only when it differs from the row's and is still registered (a lookup, no call to it); the
  source path is always there.

## The code signature

`platform/macos/signature.rs`. Read with the Security framework, never `codesign`:

- **Unsigned**: `SecStaticCodeCheckValidity` with `kSecCSBasicValidateOnly` answers `errSecCSUnsigned`.
- **Who signed it**: `kSecCodeSignatureAdhoc` in the flags is `AdHoc`; otherwise three code requirements evaluated
  against the signature alone (basic validation, no network): `anchor apple` → `Apple`, the Mac App Store leaf OID →
  `AppStore`, the Developer ID intermediate + leaf OIDs → `DeveloperId` with the team id and the leaf's summary minus
  Apple's fixed `Developer ID Application:` prefix and ` (TEAM)` suffix. Nothing parses a name to decide a kind.
- **The ticket**: a bundle's `Contents/CodeResources` file (Slack: `codesign -dvvv` reports a stapled ticket and the
  file is there, 2026-11-13); a bare executable can't carry one; a DMG keeps it inside its signature (`NotRead`).
- **The bounded check**: `kSecCSDoNotValidateResources | kSecCSCheckAllArchitectures`, on its own
  pool job, waited for `CHECK_BUDGET` (600 ms) inside the row's 1 s. Measured on macOS 27.0.1, 2026-11-13, warm page
  cache, with a Swift spike over the same calls: Slack and Xcode under 1 ms (both thin, small main executables),
  IntelliJ 8 ms, Warp's 401 MB executable 915 ms. So the check holds its budget for ordinary apps and an outsized
  executable reads "carries a signature from" with `NotChecked(TimedOut)`. Without `kSecCSCheckAllArchitectures`, a
  byte flipped in a universal binary's x86_64 slice passed on an arm64 Mac; with it the copy fails
  `errSecCSSignatureFailed`, which is `ExecutableDiffers`. A DMG's signature covers the whole image, so it's never
  checked (`NotSupported`).
- **`pkg` isn't planned**: a xar signature isn't code the API reads (crate DETAILS § The plan).

## Git

`git.rs`. Root and branch from `cmdr_git::repo_info` (on the handle the probe's look found); the status from
`cmdr_git::list_status` on its own pool job, waited for `STATUS_BUDGET` (600 ms) inside the row. Past it the root and
branch stand with the status `Unknown(TimedOut)`; the walk runs on and fills the repo's status cache, so the next card
answers at once. A cold walk is about 70 ms on a 50,000-file repo (`crates/cmdr-git/DETAILS.md` § Performance).

- **What the list proves** (`status_of`): it holds CHANGED entries only, so a clean tracked file, an ignored one, and one
  git never saw are all absent: `Unknown(NotInStatus)` (Routine), ❌ never `Clean`. A file's own `untracked` entry is
  `Untracked`; any other change (modified, a staged add or delete, a rename, a conflict) is `Tracked(Modified)`, worded
  from the side that never understates a loss. A folder with any tracked change under it holds uncommitted work; one
  with only untracked entries is `NotInStatus`, since nothing says whether git tracks the rest of it.
- **Decision: no new `cmdr-git` read.** That crate's surface is capped and a per-path status (index lookup plus the
  exclude check) needs David's say-so. With one, `Tracked(Clean)` and `Ignored` light up (the vocabulary and the removal
  rules already hold them) and only `status_of`'s source changes.
- **Gotcha: a cancelled card can't stop a cold walk.** The walk's stop flag is internal to `cmdr-git`; a cancel drops the
  answer and the thread finishes the walk (bounded by the pool's ceiling). The per-path read above would remove the walk
  altogether.
- **The relative path** canonicalizes the item's PARENT and keeps its name, so a symlink is asked about as itself.

## The owner

`owner.rs` answers the lookups `cmdr_about::owner::OwnerQuestion::answer` asks for, through `AppSource` (the OS's, or a
test's fixed answers); the rules are in `crates/cmdr-about/DETAILS.md` § The owner.

- **Installed apps**, macOS (`platform/macos/apps.rs`): every `.app` at the top of `/Applications`,
  `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, and `~/Applications`, with
  Finder's name, the bundle id, `CFBundleName`, and the folder name. Cached, rebuilt when
  `open_with::apps_changed_generation` moves (the same `NSWorkspace` launch and terminate observer the "Open with" cache
  uses) or after 30 minutes, for an install the observer can't see. ❌ Never per card. Linux: the `.desktop` entries in
  the system, user, flatpak, and snapd folders (`Name=` and the desktop id; snapd's `<snap>_<app>` also answers to the
  snap's name), aged out after 30 minutes.
- **An id elsewhere on this disk**: macOS asks LaunchServices (`installed_app_path`) and refuses an answer under
  `/Volumes` or in a `.Trash`: LaunchServices also knows apps on backup disks, mounted images, and thrown away. Linux
  looks for the flatpak or snap install folder. **Decision/Why**: the app-folder scan alone calls a helper app under
  `/Library/Application Support` (Adobe's, for one) "not installed", and its container "left over"; another volume's app
  still never counts.
- **Receipts** (macOS only, `OsCaps::install_receipts`): `pkgutil --file-info <path>` through `crate::subprocess` in the
  C locale under 800 ms. Its exit code is 0 whether a receipt lists the path or not (macOS 27.0.1, 2026-11-13), so the
  answer is the report's `pkgid:` fields, ❌ never its wording; a non-zero exit is `Unparseable`, the deadline
  `TimedOut`.

## Size and importance

`index.rs`, databases only.

- **Recursive size**: `index().dir_stats(index id, path)`, its flags verbatim (`recursive_size_complete` false is
  `LowerBound`, `recursive_size_stale` is `ExactButStale`), and `updating` from the folder's pending writes OR a walk on,
  above, or below it (`IndexStatusResponse::walk_affects` over the index status's walked roots). No stats is
  `Unknown(NotIndexed)`.
- **Importance** (folders): `tools::resources::importance::snapshot_path_on` with the store path from
  `location::store_path::to_store_path`. A score's band is `ImportanceBand::of_score`, `stale` when it predates the
  volume's latest recompute, the top three positive signals as tokens, and the visit count from the stored signals; a
  floored folder is `Floored` with its floor reason; unscored is `Unknown(NotIndexed)`.

## Tests

`tests.rs` runs the session with plain threads and a recording sink: a Chrome-shaped download (xattrs written by the
test) streams `planned`, each fact, and `factsSettled`; a plain file's download row is `Unknown(NoRecord)`; a cancel
mid-way leaves no late event and skips a queued job; a row past its budget lands `TimedOut` and its late answer drops;
a cancel before the plan sends nothing; routing sends no share item, link-reached mount, or online-only file's bytes to
a local job; a file copied in through the engine's real copy path streams its `cmdrOperation` with source and date, and
the same file saved over afterwards reads `NodeChangedSince`. `history/tests.rs` covers the wording and every way of
not knowing over a scratch journal. Through the session too: a fixture home's `Application Support/com.eltima.cmd1`
reads Commander One installed, `Slack` probably Slack, `Google` ambiguous (a fake app list, ❌ never the real
`~/Library`); a plain document plans no owner or git row; a modified repo file streams `Tracked(Modified)` on `main`.
`git_tests.rs` pins what the status list proves over a real repo, and a walk past its budget; `owner_tests.rs`,
`index.rs`, and `platform/macos/apps.rs` the lookups and mappings. `registry.rs` tests the tombstone and run ownership; `signature.rs` tests Apple's tools, a plain file, and
a tampered copy of `/bin/ls`.
