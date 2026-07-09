# Archive browsing: polish and follow-ups

The archive-browsing feature shipped 2026-07-06 (browse + edit zip, browse + extract tar family and 7z, local and remote
parents, full i18n). This spec captures everything deliberately deferred or flagged since, ranked by combined user + dev
impact (highest first). Each item stands alone; pick from the top.

Canonical docs (don't restate mechanisms here): `apps/desktop/src-tauri/src/file_system/volume/backends/archive/` C+D.md
(read core, formats, remote sources), `apps/desktop/src-tauri/src/file_system/write_operations/` C+D.md § "Archive
edits" (mutation, remote edit contract). The shipped plan (`archive-browsing-plan.md`) holds the original decisions
until it gets wiped; the durable ones are already in the colocated docs.

## 1. One-pass bulk extract for sequential archives (perf, user-visible) — SHIPPED 2026-07-08

Bulk-extracting a subtree from a compressed tar (`.tar.gz` / `.tar.xz` / `.tar.bz2` / `.tar.zst`) or a solid 7z is now
one-pass instead of O(n²): the copy planner routes a sequential directory source through
`Volume::open_sequential_extract`, which decodes the stream ONCE and materializes the wanted files as they stream by.
Per-file progress stays honest, cancel-between-entries works, and zip / plain `.tar` (random-access) keep the per-entry
path unchanged. Mechanism: archive `read/DETAILS.md` § "One-pass subtree extract" and
`write_operations/transfer/DETAILS.md` § "One-pass sequential extract".

## 2. Encrypted archives: password-prompt extraction (UX, user-visible) — ZipCrypto SHIPPED 2026-07-08, AES deferred

Extracting from a legacy PKWARE ZipCrypto zip (what macOS Archive Utility / `zip -e` produce) now works end to end:
copying or moving a source out of an encrypted zip surfaces a password prompt, stores the password per-archive, and
re-dispatches the operation so the extract decrypts. A wrong password re-prompts (caught at open, or late at
end-of-stream CRC, so the re-prompt can arrive mid-transfer too); cancel forgets the password and settles the operation.
Backend half (decrypt in the read path, the typed `ArchiveNeedsPassword` signal, per-archive password storage): archive
`read/DETAILS.md` § "Decryption" and archive `DETAILS.md` § "Password-protected archives". Frontend half (the
`ArchivePasswordDialog`, the interception + re-dispatch seam, the mid-transfer wrong-password case): transfer
`DETAILS.md` § "Archive-password prompt".

**WinZip AES zip and 7z AES still deferred.** Enabling the `aes` crate (zip `aes-crypto` / sevenz `aes256`) pulls stable
`aes 0.9.1`, which conflicts with `smb2`'s pinned `aes =0.9.0-rc.4` (its SMB3 AEAD stack) — Cargo can't unify. Both AES
kinds refuse honestly as `Unsupported` today (never "damaged", never a prompt that can't succeed): zip via the stubbed
AES branch in `zip::open_read`, 7z via `sevenz.rs::map_sevenz_err` (the unrecognized `AES256_SHA256` coder). The two
follow-ups differ in size once the `aes` versions align:

- **WinZip AES zip is close to a one-line flip:** turn on `aes-crypto` and fill the already-stubbed AES branch — the
  password plumbing (per-archive storage, prompt, re-dispatch, wrong-password detection) is the same ZipCrypto path,
  already shipped.
- **7z AES is NOT a one-line flip.** Beyond the `aes256` feature, `sevenz-rust2` wants the password at
  `ArchiveReader::new` time, so a real 7z-AES path must thread a per-archive password through `read/sevenz.rs`'s `parse`
  AND every `open_read` / `stream_subtree` re-open (each currently passes `Password::empty()`), then surface `Encrypted`
  / `WrongPassword` from `map_sevenz_err` instead of `Unsupported`. That's new parse/read plumbing, not a flag.

## 3. M-append: fast in-place zip edits (perf, research spike)

Every zip edit is an O(archive) temp+rename rewrite — safe and uniform, but adding one small file to a 2 GB zip rewrites
2 GB. True append-past-EOF (hand-rolled: append entries + fresh CD + new EOCD, old CD left as dead bytes) gives O(new
file) adds and O(CD) deletes, with real reader-compat risk (must verify Archive Utility, Quick Look, `unzip`, 7-Zip
accept the layout) and a compaction story (dead-space threshold + repack via `raw_copy_file`). The original plan's spike
notes live in `archive-browsing-plan.md` § M-append. Do this only after real archives feel slow in practice; temp+rename
is correct today.

## 4. Open-with-external-app for archive-inner files (UX)

Enter on a file inside an archive offers the built-in viewer (temp-extract, 256 MiB cap, per-instance reaper). "Open
with <external app>" is deferred: an external app needs the temp to OUTLIVE the viewer session, which means an
extract-and-persist lifecycle plus a startup reaper that doesn't exist yet (the current reaper is per-instance,
next-edit-scoped). See archive `DETAILS.md` § the deferred open-with item.

**Spiked 2026-07-08.** Recommend cloning the viewer's proven persist-extract shape (`file_viewer/archive_extract.rs`)
into a sibling open-with-persist module, with a startup-ONLY reaper (no session-close reap, because a detached launched
app has no close event to hook). Concretely:

- **Where.** A NEW per-instance dir `<app_data_dir>/open-with-extract/`, initialized at startup right next to
  `file_viewer::init_archive_extract_dir` in `lib.rs` (today `data_dir.join("viewer-extract")`, `lib.rs:458`). SEPARATE
  from the viewer dir with its OWN subdir prefix (e.g. `.cmdr-openwith-<uuid>/`) so the viewer's session-close reap
  (`session::close_session`) never removes a live open-with temp, and vice-versa. Per-instance (not shared) so
  side-by-side dev/prod/worktree instances never reap each other's LIVE temps — the exact reasoning behind the viewer
  dir. Reuse `archive_extract.rs`'s `ExtractedEntry` / `extract_entry` / `stream_to_file` shape (bounded,
  refuse-before-extract on the central-directory-declared size, streaming byte-cap zip-bomb backstop); only the reap
  lifecycle differs.
- **How long / when reaped.** The temp lives until the NEXT app startup. A launched external app holds it for an unknown
  lifetime with no close signal, so it can't be session-scoped. But the process boundary already marks every prior-run
  open-with temp abandoned (its launched app belongs to a dead session), so a startup "reap the whole dir"
  (`reap_orphan_extracts`-shaped, matching `.cmdr-openwith-*`) is both sufficient and safe — the same model as the
  viewer's crash-orphan reaper, minus the session-close reap. No age/TTL timer and no refcount (a live temp is never
  older than its own process).
- **FDA gate (`src-tauri/CLAUDE.md`).** No constraint hit: the dir is under the app data dir, which is NOT
  TCC-protected, so the startup reaper is a plain `read_dir` + `remove_dir_all` — the IDENTICAL launch operation
  `init_archive_extract_dir` already runs at `lib.rs:458`. No `is_fda_pending_runtime()` guard needed.
- **Collision / versioning.** A uuid subdir per open (like the viewer's "one temp per open"): opening the same inner
  file twice yields two independent temps, no collision, no dedup cache to invalidate on archive edit. On archive
  edit/close/navigation the temp is a detached snapshot — the launched app keeps its copy, correctly. There is NO
  write-back to the archive (inner files are read-only preview): edits the user makes in the external app are lost. Flag
  that as a product decision (read-only semantics, or a future watch-the-temp-and-mutate story), NOT a lifecycle
  blocker.
- **Menu wiring (small, reuses the existing machinery).** Launch reuses
  `file_system::open_with::open_paths_with(paths, app_path)` UNCHANGED — the only change is that for an archive-inner
  selection the launch path is the extracted temp, not the inner path. Extract on the click, in `menu_handlers.rs`'s
  `open-with:<bundle-id>` branch (`open_with::OPEN_WITH_ID_PREFIX`) and the `OPEN_WITH_OTHER_ID` branch: swap each
  archive-inner path for its freshly-extracted temp (on `spawn_blocking`, the same blocking `resolve`+stream the viewer
  uses) before `open_paths_with`.
- **One seam to verify.** Candidate LISTING at menu-build time (`commands/menu.rs::compute_open_with_choices` →
  `open_with.rs`'s `URLsForApplicationsToOpenURL:`) queries the inner path, which is NOT a real FS file. Verify whether
  LaunchServices resolves candidates for a non-existent file URL by its path extension (it generally maps
  extension→UTType without a stat, so it likely does). If it doesn't, list against a path carrying the right extension
  (the inner basename under the persist dir, no content needed) or key off the extension cache (`extension_cache_key`).
  This is the only unknown; everything else is direct reuse.

Rejected alternatives: session/refcount-scoped temp with a close hook (no close event exists for a detached app — the
very reason it was deferred); a dedup cache keyed by inner path (adds archive-edit invalidation for no real gain); an
age/TTL background reaper (redundant — the process boundary already marks prior temps abandoned); a shared (non
per-instance) extract dir (a second instance's startup reap deletes the first's live temps); write-back to the archive
on external-app save (out of scope — inner files are read-only preview).

Rough effort: small. One module cloned from `archive_extract.rs` (dir init + startup reaper + reused extract path), one
`lib.rs` init line, and the click-handler path-swap in `menu_handlers.rs`. The candidate-listing seam is the only
investigation.

## 5. Copy from remote sources INTO a zip (completeness) — SHIPPED 2026-07-08

Copying/moving a remote (SMB / MTP) source INTO a zip now works: `archive_copy_into_start` runs a source-side pull-to-
scratch prologue (via the copy engine's `pull_path_to_local` seam) before the ordinary local ingest, orthogonal to the
archive parent's local-vs-remote handling. Move deletes the remote originals after the durable commit. Canonical docs:
`write_operations/DETAILS.md` § "Archive edits" (source-side pull bullet).

## 6. Remote-backed archive live refresh (UX, niche)

A LOCAL archive listing live-refreshes when the backing file changes on disk (real `notify` watch on the `.zip`). A
REMOTE archive doesn't (no watch transport over SMB / MTP). Options: poll `get_metadata` (size + mtime) on a
visible-pane cadence, or accept manual refresh as the contract. Today's behavior is stale-until-refresh with no
indicator.

**Spiked 2026-07-08.** Recommend SMB: reuse the existing recursive CHANGE_NOTIFY watcher (push, not poll). MTP: accept
manual refresh (F5) as the contract.

**SMB — reuse `smb_watcher.rs`, no poll.** The SMB watcher already opens CHANGE_NOTIFY on the share root RECURSIVELY for
the whole volume lifetime (it feeds the drive index), so it ALREADY receives a `FileNotifyAction::Modified` for any
changed `.zip` and forwards it to `notify_directory_changed(volume_id, parent_dir, DirectoryChange::Modified)`. That
refreshes the DIRECTORY listing (the `.zip`'s new size/mtime) but not the archive-INNER listings. The fix: in the
smb_watcher Modified/Renamed handlers, when the changed path has a supported archive extension, ALSO call
`caching::refresh_archive_listings(volume_id, &zip_path)` — the exact refresh the local `archive/watch.rs` already
fires. It's a no-op when no inner listing is open (it scans `LISTING_CACHE` for keys at/inside the archive path), and
`volume_id` here IS the parent drive id, which is what archive listings key on, so it lines up with no rekeying. Cost is
near-zero: the watcher already runs, and the re-parse only happens when the `.zip` actually changes AND an inner listing
is open.

- **Keep `ArchiveVolume::listing_is_watched` FALSE for a remote parent regardless.** The SMB watcher is documented
  lossy-under-load (`backends/CLAUDE.md`, `volume/CLAUDE.md`), so the write-op fresh-listing oracle must still re-read
  pre-flight scans honestly. The push-refresh above is a VISIBLE-listing UX nicety; it's a SEPARATE consumer from the
  data-safety oracle — don't conflate them by flipping `listing_is_watched` to true.

**MTP — manual refresh (F5).** No poll, no watch. Rationale: `MtpVolume::get_metadata` lists the ENTIRE parent directory
(MTP has no single-file stat — `backends/CLAUDE.md`), so a visible-pane metadata poll is expensive per tick; the MTP
event loop's `ObjectInfoChanged` is absent on many devices (cameras especially — `mtp.rs` `listing_is_watched` note), so
it's not a dependable transport; MTP zip editing is itself a stretch (item 9 / M6); and an out-of-band rewrite of a
`.zip` on a connected device while it's being browsed is rare. The `(path, size, mtime)` index-cache key already forces
a re-parse on the next navigation/refresh, so a stale render never outlives an F5. If the MTP event loop DOES already
emit an `mtp-directory-changed` for the `.zip`'s object on a given device, hooking the same `refresh_archive_listings`
opportunistically is a cheap bonus — but don't build a poll and don't promise freshness (`listing_is_watched` stays as
is).

Rejected alternatives: polling `get_metadata` on a visible-pane cadence for SMB (strictly worse than the CHANGE_NOTIFY
we already receive — adds latency and periodic round-trips for a push signal that already exists); polling for MTP (a
full parent-dir listing per tick for a rare event); flipping SMB archives to `listing_is_watched = true` (would let the
write-op oracle trust a lossy watcher's cache for pre-flight sizing — a data-safety regression).

Rough effort: SMB small (a few lines in `smb_watcher.rs` plus the reused `refresh_archive_listings` call); MTP zero
(document the manual-refresh contract; the cache key already guarantees correctness on the next read).

## 7. Remote edit temp reaping (data hygiene, small)

A remote zip edit uploads under a temp name and swaps. A crash/cancel exactly between upload and swap can leave the temp
under its remote name; local leftover temps get reaped at the next edit of the same archive, remote ones don't
(documented non-reap). A next-edit-of-same-remote-archive reap (list siblings, delete `*.cmdr-tmp-*` older than a
threshold) closes it. Small, bounded, and the crash window already never loses NEW data.

## 8. Move-out per-source delete convergence (edge-case correctness) — SHIPPED 2026-07-08

Move OUT of a zip now converges: the batch `{ delete }` drops exactly the top-level sources that extracted in FULL
(durable, zero deep skips), so a partially-interrupted move deletes the moved subset and a retry handles the remainder
instead of restarting from zero. A skipped or errored source stays in the archive; a hard error deletes the durable
prefix; cancel/rollback delete nothing (cancel matches the plain cross-volume move). This also fixed a latent DATA-LOSS
bug: a deep-merge Skip inside a directory source was uncounted, so the old all-or-nothing gate saw zero skips and
deleted the whole subtree including the un-landed child. The copy engine now folds `CreatedPaths::skipped_file_count`
into `files_skipped` and reports fully-extracted sources via `note_source_landed_clean`. Mechanism: `write_operations`
`DETAILS.md` § "Archive edits".

## 9. M6: MTP in-place editing (stretch, cross-repo)

In-place remote editing for MTP devices (today: pull-edit-upload-swap, which is correct but O(archive) over USB both
ways). Touches `mtp-rs` (first-party). Stretch from the original plan; only worth it if MTP zip editing sees real use.
Notes: `archive-browsing-plan.md` § M6.

## 10. Dev-side debt (warns and tight margins)

- `file-length` growth warns needing trim-or-consent: `transfer/volume_copy_tests.rs` (2461 lines, allowlist 2102),
  `app.css` (1579, allowlist 1202), `indexing/manager.rs` (1289, allowlist 1147), `listing/caching_test.rs` (1304,
  allowlist 1168), plus ~14 unlisted files newly over 800. Split/trim where it's an architecture win; otherwise ask
  David for allowlist consent explicitly.
- Four archive-area `CLAUDE.md`s sit at 595–599 words against the 600 ceiling (volume/, backends/archive/,
  write_operations/, pane/). The next few added sentences re-trip the warn; the honest fix then is a folder split
  (backends/archive/ is the candidate: 20+ files spanning read core, formats, and mutation), not another squeeze.
- ~~`smb.spec.ts` `recreateFixturesAndSettle()` uses a rationale-documented `sleep(1000)`.~~ Resolved: the settle it
  provided is already done by the `ensureAppReady()` both call sites run right after it (navigates, polls the fresh
  `left/` fixtures present, calls `flushFileWatcher()`, then re-confirms stability — see `helpers/app-lifecycle.ts`
  ~L158), so the `sleep` was redundant. Dropped it: both call sites now run a bare `recreateFixtures(getFixtureRoot())`
  and the `recreateFixturesAndSettle()` helper is gone. Verified on the SMB Docker E2E lane: both cross-storage copy
  tests exercised the new path and `ensureAppReady()` settled cleanly (left pane populated to its 11 entries with no
  watcher race, which would have thrown). The copy assertions themselves skipped in the local run on a standing
  gvfsd-fuse mount-visibility quirk (`fs.existsSync(SMB_GUEST_MOUNT_SUITE)` false locally; the path resolves in CI) —
  orthogonal to the sleep removal.
- ~~One E2E duration warn: the cancel-paste spec runs ~4.4 s (suite target: well under a second per test).~~ Resolved
  (investigated, left as-is with a rationale): no stray sleep. The duration is inherent to what it pins — a mid-transfer
  cancel needs a transfer long enough to catch mid-write, so the 24 MB write + zip-compress window is load-bearing. A
  guardrail comment in the test now says not to shrink it. Documented in `archive-browsing.spec.ts`.

## 11. Awaiting David (no agent action)

- QA nits from his visual pass (Enter menu, Archives settings, queue rows, delete confirm, damaged-archive banner).
- English copy review, incl. the two raw Rust backstop strings.
- Translation terminology flags: zh/vi compressed-file overrides, nl "App-pakketten", es "cifrado", hu "Rákérdezés", sv
  "Appaket", vi "bản mới".
- The `quick-xml` cargo-audit advisory is Renovate's to close (transitive dep, advisory published 2026-07).
