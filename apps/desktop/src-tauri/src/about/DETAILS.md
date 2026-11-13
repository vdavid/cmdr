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
- `volumeStoppedAnswering`: at most once, for a card reached through its volume whose identity call (the health check)
  didn't answer within `VOLUME_BUDGET`. The card's volume note switches to "stopped answering" in the slot it already
  had, and every row still waiting lands `Unknown(VolumeStoppedAnswering)` (Routine) right after.
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

`about_start` is registered dispatch-only (raw invoke: a `Channel` specta can't describe); `about_cancel` and
`about_stop_verdict` are typed, and `apps/desktop/src/lib/tauri-commands/about.ts` wraps all three (the card's handle:
`cancel`, `stopVerdict`).

## Routing

`route::gather_for(request, started)` resolves through `crate::location::resolve` (a miss is `unavailable`, kept whole
as `NoCard::miss` for the agent's lookup; a search-results pane's
volume id is dropped first, since its rows are real files on whatever drive holds them), then picks a `route::Way` by
the volume's kind, whether it's live, and whether its paths are the OS's own (`Volume::paths_are_os_visible`: what
tells a share macOS mounted from one Cmdr dials). A path the lexical gate (`path_routes_over_its_parent`) lets through
asks the volume manager which view serves it, on the about pool under `PROBE_BUDGET` (a route's confirm can stat the
drive): an archive, git's history, or a backup snapshot.

The table (`route::caps_for`, pinned by `through_tests.rs::each_way_asks_only_what_it_can_answer`):

- **`Local`** (a live local disk): every row, below.
- **`OsMountedShare`** (Finder's SMB, an OS NFS or WebDAV mount): identity through `get_metadata`, the download record
  through `read_xattr_many` (capped as a local read is), and the index rows (size, What's inside, importance) when a
  live index holds the share. No Spotlight, signature, byte read, owner, git, or operation log: each would be a slow
  read over the network, or a walk.
- **`Server`** (SMB, SFTP, WebDAV, S3, NFS that Cmdr dials): identity, and the index rows.
- **`Device`** (MTP, ADB): identity only.
- **`Archive`**: identity from the archive's own listing, and the download row naming the archive with ITS record
  (read here when the archive sits on a local disk, through the share's xattrs on an OS-mounted share, else "Cmdr can't
  tell"). No note: the row says where it comes from.
- **`GitHistory`** (`.git/branches|tags|commits|stash/…`): identity and the snapshot row from the portal volume. The
  branch's or tag's name is the shortest run of path components the portal answers for (`feature/foo` holds a slash);
  no author, which needs a per-entry `cmdr-git` read its capped surface doesn't have.
- **`Snapshot`** (a restic view): identity only.
- **`Offline`**: `unavailable { noVolumeAtPath }`.

### What the footer's actions may do, by way

`route::action_caps(way, volume, path, home)` fills `cmdr_about::actions::ActionCaps` once, before any job, and the
session hands it to every derive (pinned by the `through_tests.rs` action-caps tests):

- **Show in the file manager**: `Local` and `OsMountedShare` only, where the OS sees the path itself.
- **Remove**: only on a volume that takes writes (`Volume::is_writable`); `Trash` where it keeps one
  (`Volume::has_trash`), else `Delete`. Both open the pane's delete dialog, which asks.
- **❌ Neither** for `Archive`, `GitHistory`, `Snapshot`, or `Offline`: an entry inside a zip has no Trash and Finder
  can't show it; git's history and a backup snapshot are read-only views.
- **The app's data folder**: `Local` only, from `OwnerQuestion::app_folder_above` and the home folder.
- **A card whose volume already stopped answering** (`Gathered::silenced`) gets no actions: a reveal or a delete
  dialog on a dead share would wait as long as it does.

**The home folder** (`route::home_folder`) is the person's, or in a dev or E2E build `CMDR_ABOUT_FIXTURE_HOME`
(`test_mode::about_fixture_home`), so the QA fixture folder's fake `Library` reads like the real one without a write
into it. It moves only the owner row's and the app-folder action's idea of home.

### A volume that stops answering

**Decision: a card reached through its volume is planned from the pane, before the volume is asked anything.** The
request carries the pane's `isDirectory`, and `route::hinted_target` reads it with the name's package suffix. Why: a
plan that waits for `get_metadata` waits as long as a dead NAS does (minutes), and the card's first frame waits with
it. The identity row is then the health check (`session::Watch::TheVolume`): still out at `VOLUME_BUDGET` (2 s), the
session sends `volumeStoppedAnswering` and lands every waiting row `VolumeStoppedAnswering`, under one lock so an
identity landing at the last moment is never followed by "stopped". The index rows answer from their databases either
way. A view's rows (`Watch::EachRow`) time out one by one instead: they read one file or repo, and a slow one is slow by
itself.

**Every volume call runs to its end on an about-pool thread** (`through::Asked::ask`, `Handle::block_on`), ❌ never
awaited on the executor or dropped by a bare timeout: an abandoned MTP transaction wedges the phone
(`crates/cmdr-fs/src/deadline/CLAUDE.md`). `through::Stuck` counts each volume's calls from start to real return;
with `MAX_STUCK_PER_VOLUME` (2) out, the next card asks that volume nothing (`Gathered::silenced`): its rows land at
once and the note says it stopped. That bounds a dead share to two stranded pool threads, however often the card opens.

### A local item

- **`Local`**: `probe` on the about pool under `PROBE_BUDGET` (1.5 s; past it, `Unreachable`). The probe is one `lstat`,
  a mount-table read, for a dotted folder one `NSURLIsPackageKey` lookup, and on a local disk the look for a repo around
  the item (`git::repo_around`, through the app's repo cache), so the plan knows whether it has a git row.
  - The route also asks `OwnerQuestion::of(path, home)` (pure, no IO), so the plan knows whether it has an owner row,
    and places the item in the index (`index::IndexPlace`: the index id, which isn't always the volume's, and the
    importance store's path for it).
  - The mount table must call the item's mount a local disk (`mount_is_local_disk`). A path whose parent chain reaches a
    share through a symlink resolves to the boot volume and isn't local: it answers identity only.
  - `SF_DATALESS` (an evicted cloud file) sets `contents_local: false`, so the plan has no real-type or signature row.

## Jobs and budgets

`jobs::GATHERED` is every kind: identity (from the probe), real type, bundle, signature, download, Cmdr's own history,
last opened, default app, importance, the owner, git, a folder's whole size, its digest, and an app's Library data. A
row whose store isn't there is trimmed from the plan (`jobs::Sources`: no operation log, no index place, no home). Each row has `ROW_BUDGET` (1 s)
from the start of the gathering; a row still out then lands `Unknown(TimedOut)` and its job's late answer is dropped.
The `about_facts` pool (4 target, 12 max workers) bounds how many threads a dead mount can strand. Its
`spare_after` (250 ms) stops a worker that's slow on one job from counting toward the four, so one card's slow rows (or
the agent's burst of lookups) can't make the next card's quick rows queue past their budget; how the pool grows:
`apps/desktop/src-tauri/src/file_system/DETAILS.md` § Threading.

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
  file is there, 2026-11-13); a bare executable can't carry one; a DMG keeps it inside its signature (`NotRead`). The host builds every signature through `cmdr_about::payload::Signature::read`, which answers `DoesNotApply` for Apple's, the Mac App Store's, ad hoc, and unsigned code: a missing ticket there is no news.
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

## A folder's digest and an app's data

`digest.rs`, the index and the search arena only. ❌ No walk, `stat`, or listing of the disk: a 2 TB folder answers as
fast as an empty one, and a container is never opened. Rules in `crates/cmdr-about/DETAILS.md` § A folder's digest.

- **The digest always answers**: the class comes from the folder's name first, and only `children`, the largest files,
  and the untouched share say `NotIndexed` where the index hasn't listed the folder.
- **Children**: `Index::list_children` (`None` for a folder nothing listed is `NotIndexed`: rows under it are a lower
  bound, not its contents), then one `dir_stats_batch` for the folder children, each size's flags verbatim and
  `updating` from the walked roots. The class asks the parent's listing for project files, once, only for a name that
  needs one.
- **The two searches run on pool jobs of their own, beside the listing, under `SEARCH_BUDGET` (800 ms) inside the row's
  1 s**: each scans the whole arena whatever the folder's size. Measured with `apps/desktop/src-tauri/src/search/bench_card.rs` (release build,
  macOS 27.0.1, a real 8,028,071-row `index-root.db`, 2026-11-13, a loaded machine): the largest files 400–860 ms
  (home, a projects folder, `~/Library`, `/Applications` alike), the untouched ones 50–100 ms. So on a big drive the
  digest lands after the 100 ms grace (the card reserves the section's whole height, frontend DETAILS § Layout), and on
  a busy machine the largest files can say `TimedOut` while the listing stands. A scoped scan that skips rows outside
  the folder would make it instant; that's a search-engine change, not the card's.
- **Largest files and the untouched share**: a size-sorted search of the folder's files through
  `search::run_if_loaded`, which answers ONLY when the volume's arena is already in memory (a load is seconds and
  hundreds of MB). No arena, or the folder not in it, is `NotIndexed`. System-folder excludes are off, or a
  `node_modules` card would hide its own files. The share sums the largest 1,000 untouched files and says "at least"
  past them or when the person's excludes hid some.
- **An app's data**: the bundle id from the app's own `Info.plist`, then each `APP_DATA_PLACES` folder of the home's
  index (`Index::volume_id_for_path(home)`), filtered by `app_data_place`, sized by one `dir_stats_batch`. A place the
  index hasn't listed is named in `not_looked_in`.

## Linux

What the card answers there, verified in the Linux E2E container (`about-this.spec.ts`, Ubuntu 26.04 arm64,
2026-11-13):

- **The download record** from `user.xdg.origin.url` / `user.xdg.referrer.url` (`platform/linux.rs`).
- **No signature row**: `OsCaps::code_signing` is off, so the plan leaves it out (❌ never drawn as "not applicable").
- **No default-app row**: Cmdr's Linux path only launches through `xdg-open` (`file_system/default_app.rs`) and asks
  nothing about which app that is, so `OsCaps::default_app` is off. A `xdg-mime query default` lookup would be a
  subprocess per card under a deadline; not built.
- **No last-opened or receipt rows**: no Spotlight, no receipts.
- **The owner's Linux arms**: `~/.config`, `~/.local/share`, `~/.cache` children named after a desktop entry read as
  probably that app's; `~/.var/app/<id>` and `~/snap/<name>` by id.

## What a model gets

`model.rs::for_model(card, path, gate)` is the app's only way to a card's model projection
(`crates/cmdr-about/DETAILS.md` § The model's projection holds every rule): it passes the gate's answer and the app's
own size and date words (`human_size`, `format_local_date`). The agent tool and the verdict call it; ❌ neither calls
`AboutCard::for_model` itself, so neither can skip the gate.

- **The gate is a type**: `Gate` is made only by `model::gate` / `gate_within`, so a projection can't be built on a
  path nobody checked.
- **At least as strict as `inspect_file`**: `GateReach::FollowLinks` is `sensitive::path_gate_within` (the rules on
  the path, then on its realpath, so `~/keys/id_ed25519` with `keys → .ssh` is protected), on its own thread under
  `PROBE_BUDGET`, ❌ never the about pool: queued behind the card's own slow jobs (Spotlight, receipts), it timed out on
  a healthy disk in tests. Past the budget it fails closed: `ProtectedReason::Unresolved`, name-level facts only.
- **`GateReach::NamesOnly`** (`sensitive::names_gate`, no realpath) only for a card that asked the volume nothing
  (`route::Asking::RefusedInBackground`): every row left comes from the drive index, which stores the paths it walked,
  so the path as given is what they describe. `Asking::gate_reach` ties the two, so a card that reads the volume always
  follows links.

Its tests: a credentials path and a link into one leave name-level facts, a gate that can't finish fails closed, and
numbers read as other tool results word them.

## The agent's lookup

`lookup.rs::look_up(path, volume_id, started, cancel)` backs `about_item`. ❌ No second gathering path: it calls
`route::gather_for` and `session::run` with the card's `Timing::CARD` and pool, through a `Kept` sink that rebuilds the
card from the stream instead of sending it, and runs `model::gate_within` beside it.

- **Registered like a card** (`about-item-<uuid>` in `REGISTRY`), so quitting stops it.
- **`LOOKUP_DEADLINE` (5 s)** is past every card budget (`PROBE_BUDGET`, then `ROW_BUDGET` or `VOLUME_BUDGET`), so by
  then the session has settled each row, `TimedOut` included. A row still out is one whose job never answered: it
  stays `pending` and is named in `still_out`, ❌ never dropped or waited on. Then the token trips, so late answers drop.
- **The turn's Stop** ends it at once (`biased` select: a stop that's in wins over an answer that's ready) as the
  location contract's `cancelled` miss; any place that can't be described is that contract's miss too
  (`route::NoCard::miss`).
- **`Started`** comes from the caller: a wake or a follow-up is `ByCmdr`, so `gather_for` asks no network or device
  volume and no backup view (a restic repository can sit on a server): those rows answer `RefusedInBackground`, the
  index rows still run. A local disk answers in full. The decision sits before the route's confirm, which reads the
  drive.
- **A local path whose parents link into a share counts as the share** (`probe::reaches_a_share`). The resolver places
  `~/nas/report.pdf` (with `~/nas` → `/Volumes/naspi`) on the boot disk by its text, and the probe's `lstat` would then
  reach the share through the parent's link. So a `ByCmdr` card walks the path from the root first: each component is
  checked against the mount table (`getfsstat(MNT_NOWAIT)` / `/proc/mounts`, no call into any mount) before it's
  `lstat`ed, and each link is read where it lives, on the local disk. A component under a mount that isn't a local disk
  makes the card `RefusedInBackground` the way a share's own path does (index rows only, names-only gate). A link loop,
  an unreadable mount table, or a walk past `PROBE_BUDGET` reads as a share: a background card that can't prove the
  path local asks nothing. The item itself is never followed (it's `lstat`ed as itself). Residual: the walk `lstat`s
  each LOCAL ancestor and `readlink`s each local link, both on the local disk; a person's own card (`ByPerson`) doesn't
  walk, since its `lstat` reaching the share is what the person asked for. Tests: `probe.rs` (a link into a stand-in
  share, absolute and relative, found with no `lstat` inside it).
- **No pane, so no `isDirectory`**: an item reached through its volume is planned from the drive index's answer
  (`route::is_directory`, `dir_stats` on a blocking thread); one the index doesn't hold plans as a file.

Tests: `lookup.rs` (a row still out at the deadline is named and `pending`; a stop ends it), and the tool through the
real gate in `apps/desktop/src-tauri/src/tools/tests/about_item_tests.rs` (facts on a user turn, `directModeOff`, the direct read logged by tool,
a wake making zero calls to a share Cmdr dials, a protected path and a link into one).

## The verdict

`verdict/`. Two or three plain sentences a model writes from the card's projection and a small look inside the file,
streamed on the card's own channel beside the facts. The prompt and its safety reasoning: plan D3, D4, D8, D13, D17,
D18 (`docs/specs/tell-me-about-this-plan.md`, until the wipe).

### Its events

After `planned`, interleaved with the facts: `verdictPlanned` (reserve the block) or `verdictUnavailable { why }` (no
block; `aiOff`, `turnedOff`, `localOff`, and `modelOff` draw nothing in the body), then `verdictGiven { contents }` right BEFORE the call goes out (so the
card says what's leaving while it leaves), `verdictDelta { text }`s (plain text: render as a text node), and one
`verdictEnd { ending, cost }` (`done`, `stopped`, or `failed { problem }`; `cost` only when the call reached its end).
A reopen on the same item gets `verdictPlanned` then `verdictFromEarlier { contents, text }` instead, with no call.
`watched.rs` tees the card's sink: it forwards every fact update, keeps the card as it stands, and wakes the verdict on
`planned` (only after `planned` is on the channel, so the verdict's first word always follows it) and on
`factsSettled`. A closed card (its token tripped) hears nothing more, not even `verdictEnd`.

### The order, which is the consent guarantee

1. **The gate** (`gate::resolve_for_card`, on a blocking thread beside the card's placing): `ai.provider` off →
   `aiOff`; `aboutThis.aiVerdict` false → `turnedOff` (`settings::load_about_this_ai_verdict`, absent = on); Local →
   `localOff` while `VERDICT_ON_LOCAL` is false (D4: flipping it is that one constant); then
   `resolve_agent_llm(app, AgentSlot::AboutThis)`, whose refusals map to `noCloudConsent`, `managed { refusal }`,
   `notConfigured`, or `notReady` (a local server that isn't running: a verdict never starts it or a download). Then
   the model the slot named, against the eval's table (`gate::writes_verdicts`): only a model in `PASSED_THE_EVAL`, on
   the provider it was measured through (a dated snapshot of its id counts), writes; a family in `FAILED_THE_EVAL`
   (Haiku, in any provider's spelling) never does; anything else is unmeasured and follows `UNMEASURED_MODELS_WRITE`
   (off). Each refusal is `modelOff`, which draws nothing, like `localOff`. **Decision: unmeasured means off** (the
   lead's default, pending David): the bundled 3B model showed an unmeasured model can hand out green lights and follow
   injections, and nothing at runtime would catch it. Letting unmeasured cloud models write on the no-removal prompt is
   that one constant. Adding a model is one row, after the eval:
   `docs/notes/about-verdict-eval.md` § Measuring a new model. No `main.db` ledger row → `notReady`, ❌ never an
   unmetered call. Only an `Ok` builds a `gate::Ready` (the LLM, the call settings, the prompt variant, the meter), and
   everything after takes one.
2. **The reuse and the ceiling** (`limits.rs`): a finished verdict for the same volume, path, node, size, mtime,
   provider, and model comes back from memory (32 entries, never disk). Else at most `MAX_STARTS` (12) in any 60 s,
   past which `tooManyRequests`. Then `verdictPlanned`, and the one in-flight slot: a new verdict stops the old.
3. **The facts**: it waits for `factsSettled` or `FACTS_CAP` (1.5 s from the start), whichever first, and the user
   message names the rows still out (`stillGathering`; their facts read `pending`).
4. **The sensitive gate and the look inside**: `model::gate_within` (as the agent tool), then, only for a file on a
   local disk whose bytes are there (`session::ItemReach::bytes_readable`, set by the route), ONE `inspect_path` call
   on its own thread under `PEEK_BUDGET` (2 s), cut by `peek::shape` to 40 lines / 2,000 characters, a PDF's title and
   first page, 20 archive names, or a photo's make, model, lens, and date. ❌ `Peek::CameraDetails` has no field for
   GPS, so the location can't leave (D8). A folder (its digest is the look), an item reached through its volume, or an
   online-only file gets none. A protected path gets none and logs the refusal by tool and reason
   (`ContentTool::AboutVerdict`), ❌ never by path.
5. **The prompt** (`prompt.rs`): the system prompt is one of two static files in `agent/prompts/` (`about_verdict.md`
   with the removal sentence, `about_verdict_no_removal.md` without), picked per model class by
   `PromptVariant::for_provider`. **Decision: no class words removal** (D17 (b)): asked to, every Claude class the eval
   measured gave green lights ("removing it would lose only the download record"), and without it Sonnet and Opus held
   the bar. A class that later passes with it flips back there. The user message
   is the language line, a header saying everything below is data written by whoever made the item, and ONE fence
   holding the projection (`model::for_model`), the rows still out, and the peek; every marker inside is defanged, so
   the content can't close the fence.
6. **The call**: `AgentLlm::respond` with no tools, built with no LLM log context (`into_unlogged_llm`: the prompt holds
   a path and facts, the answer is untrusted). `resolve_verdict_call_settings`: a 120-token reply, thinking off where
   the model can stop, else its lowest level with 2,000 tokens of room (evidence on `VERDICT_THINKING_ROOM`).

**Worst case, stated**: with no tools the model can't act; a hostile item can at most make the block say something
false or reassuring in plain text. That's why the actions, the removal row, and "Continue in Ask Cmdr" never depend on
the verdict's text, and why there's no runtime phrase filter (it would miss a paraphrase and every other language).

### Stopping it

- `about_cancel` (the card closed) trips the card's token, and the verdict's token is its child.
- `about_stop_verdict(requestId)` stops the verdict alone; the card stays and the text so far stands (`stopped`).
- `ai::cloud_consent::stop_in_flight_calls` (a revoke, a held "no", a narrowing managed policy) calls
  `about::stop_verdicts`, which stops the one in flight. The LLM client's own per-request policy check is the backstop.
- Quitting: `about::cancel_all` trips every card's token.

### Its cost

A finished call meters through `agent::pricing::price_call` + `record_cost` onto the reserved `about_this` conversation
row (store migration v15, `ConversationOrigin::AboutThis`), on a blocking thread. The row holds cost only: ❌ no text,
prompt, or path reaches `main.db`. It's hidden from the session list and search with the quiet-wakes row
(`ConversationOrigin::LEDGERS`) and outside the wake loop's daily ceiling. An unpriced model records `priced = false`,
shown unknown, never $0. **A stopped or failed call reports no usage** (the provider's usage rides the stream's `End`,
which a dropped stream never reaches, the same as a stopped rail turn: `agent/chat/DETAILS.md` § Reporting what a turn
cost), so nothing is metered for it and its `verdictEnd` carries no cost. A call that ends with no text (a reasoning
model that spent its reply thinking) ends `failed { noAnswer }` WITH its cost, and isn't cached.

### The E2E fake

Under `CMDR_E2E_ASK_CMDR_FAKE`, `AgentSlot::AboutThis` plays `e2e_fake::ABOUT_THIS_FAKE_VERDICT` (fixed text, four
deltas) for every card; the gate's provider and setting steps still apply, so a spec sets `ai.provider` to `cloud`.
`CMDR_E2E_ABOUT_VERDICT_PACE=<ms>[,<hold ms>]` paces it (`FakeAgentLlm::paced`): a wait before each delta and a hold
before the last, so the streaming and stopped states stay on screen for QA and screenshots.

### The eval on real models

The verdict's only gate on its no-green-light rule is `apps/desktop/src-tauri/src/ai/about_verdict_eval_test.rs`: 51 synthesized cards (ordinary,
tempting, hostile) through the shipped projection, fence, prompt files, and call settings, on three Claude classes and
the local model, in five languages, graded by an Opus judge against `apps/desktop/src-tauri/src/ai/about_verdict_eval/rubric.md`. ❗ Rerun the
whole corpus after any change to a prompt file, `for_model`, the fence, the call settings, or a default model.
Results, the decisions they drove (no class words removal, the Haiku family writes none, Local stays off), and every
flagged take: `docs/notes/about-verdict-eval.md`.

Before it, a hand run on `claude-haiku-5-5` at the model's default effort under 120 tokens wrote nothing at all, which
is what `VERDICT_THINKING_ROOM` and `noAnswer` answer (verified 2026-11-13, four cards).

## Tests

`verdict/tests.rs` drives the verdict over a recorded channel with the fake LLM and a counting reader: the order
(`verdictPlanned` before the facts settle, `verdictGiven` before the first delta, the cost at the end), every gate
refusal reading no file and calling no model, the cap naming rows still out, Stop and a revoke ending `stopped` with
the card open, a closed card hearing nothing, a reopen reusing from memory, the 13th start refused, a protected item
going out name-level with nothing read, a folder read never, a provider failure and an empty answer ending typed.
`verdict/peek_tests.rs` runs the real `inspect_file` core (GPS never leaves, the window's caps, a protected file, a read
past its budget); `prompt.rs` the fence (a marker in a name can't close it); `limits.rs` the ceiling, the one slot, the
cache; `gate.rs` the refusal map and the ledger row holding cost only. The revoke and narrowing paths:
`ai/cloud_consent.rs` and `ai/managed.rs` tests. Pool starvation: `tests.rs::a_slow_card_cant_starve_the_next_cards_rows`.

`through_tests.rs` runs a card reached through its volume end to end: the `Way` table, a scripted share whose every
call waits (the card plans at once, says it stopped once, settles inside the budget, and the next card asks it nothing),
a healthy `InMemoryVolume` share's identity and xattrs, an item gone since the listing, an archive's entry naming its
archive with the archive's record, and a file in a slashed branch's snapshot over a real portal (the category words
pinned against `GitPortal::category_rows`).

`tests.rs` runs the session with plain threads and a recording sink: a Chrome-shaped download (xattrs written by the
test) streams `planned`, each fact, and `factsSettled`; a plain file's download row is `Unknown(NoRecord)`; a cancel
mid-way leaves no late event and skips a queued job; a row past its budget lands `TimedOut` and its late answer drops;
a cancel before the plan sends nothing; routing sends no share item, link-reached mount, or online-only file's bytes to
a local job; a file copied in through the engine's real copy path streams its `cmdrOperation` with source and date, and
the same file saved over afterwards reads `NodeChangedSince`. `history/tests.rs` covers the wording and every way of
not knowing over a scratch journal. Through the session too: a fixture home's `Application Support/com.eltima.cmd1`
reads Commander One installed, `Slack` probably Slack, `Google` ambiguous (a fake app list, ❌ never the real
`~/Library`); a plain document plans no owner or git row; a modified repo file streams `Tracked(Modified)` on `main`.
`digest_tests.rs` reads a drive index the test writes: coverage flags carried, an unlisted folder `NotIndexed` with
its class kept, an empty one known and empty, `node_modules` and a Rust `target` classed, an app's Library folders found
by id while a name-only vendor folder and another app's cache stay out. `git_tests.rs` pins what the status list proves over a real repo, and a walk past its budget; `owner_tests.rs`,
`index.rs`, and `platform/macos/apps.rs` the lookups and mappings. `registry.rs` tests the tombstone and run ownership; `signature.rs` tests Apple's tools, a plain file, and
a tampered copy of `/bin/ls`.
