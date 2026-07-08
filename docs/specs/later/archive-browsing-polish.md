# Archive browsing: polish and follow-ups

The archive-browsing feature shipped 2026-07-06 (browse + edit zip, browse + extract tar family and 7z, local and remote
parents, full i18n). This spec captures everything deliberately deferred or flagged since, ranked by combined user + dev
impact (highest first). Each item stands alone; pick from the top.

Canonical docs (don't restate mechanisms here): `apps/desktop/src-tauri/src/file_system/volume/backends/archive/` C+D.md
(read core, formats, remote sources), `apps/desktop/src-tauri/src/file_system/write_operations/` C+D.md § "Archive
edits" (mutation, remote edit contract). The shipped plan (`../archive-browsing-plan.md`) holds the original decisions
until it gets wiped; the durable ones are already in the colocated docs.

## 1. One-pass bulk extract for sequential archives (perf, user-visible) — SHIPPED 2026-07-08

Bulk-extracting a subtree from a compressed tar (`.tar.gz` / `.tar.xz` / `.tar.bz2` / `.tar.zst`) or a solid 7z is now
one-pass instead of O(n²): the copy planner routes a sequential directory source through
`Volume::open_sequential_extract`, which decodes the stream ONCE and materializes the wanted files as they stream by.
Per-file progress stays honest, cancel-between-entries works, and zip / plain `.tar` (random-access) keep the per-entry
path unchanged. Mechanism: archive `read/DETAILS.md` § "One-pass subtree extract" and
`write_operations/transfer/DETAILS.md` § "One-pass sequential extract".

## 2. Encrypted archives: password-prompt extraction (UX, user-visible)

Today: browsing an encrypted zip works, extraction returns a typed `Encrypted` refusal; 7z AES stays off (`sevenz-rust2`
`aes256` feature). Users do hit password-protected archives in the wild, and the current experience is a dead end (an
honest one, but still a dead end). Shipping this means: a password prompt flow (dialog + retry on wrong password +
remember-for-this-archive), decrypt support in the read path (7z: flip the `aes256` feature; zip: `rc-zip` does NOT
decrypt, so this needs a decrypt layer or another crate — spike first), and the mutation-side interaction is already
settled (edits that would RETAIN an encrypted entry stay refused; see archive `CLAUDE.md`). Biggest effort of the
user-facing items; rank it by demand signals from beta users.

## 3. M-append: fast in-place zip edits (perf, research spike)

Every zip edit is an O(archive) temp+rename rewrite — safe and uniform, but adding one small file to a 2 GB zip rewrites
2 GB. True append-past-EOF (hand-rolled: append entries + fresh CD + new EOCD, old CD left as dead bytes) gives O(new
file) adds and O(CD) deletes, with real reader-compat risk (must verify Archive Utility, Quick Look, `unzip`, 7-Zip
accept the layout) and a compaction story (dead-space threshold + repack via `raw_copy_file`). The original plan's spike
notes live in `../archive-browsing-plan.md` § M-append. Do this only after real archives feel slow in practice;
temp+rename is correct today.

## 4. Open-with-external-app for archive-inner files (UX)

Enter on a file inside an archive offers the built-in viewer (temp-extract, 256 MiB cap, per-instance reaper). "Open
with <external app>" is deferred: an external app needs the temp to OUTLIVE the viewer session, which means an
extract-and-persist lifecycle plus a startup reaper that doesn't exist yet (the current reaper is per-instance,
next-edit-scoped). Design the temp lifecycle first (where, how long, when reaped), then the menu wiring is small. See
archive `DETAILS.md` § the deferred open-with item.

## 5. Copy from remote sources INTO a zip (completeness) — SHIPPED 2026-07-08

Copying/moving a remote (SMB / MTP) source INTO a zip now works: `archive_copy_into_start` runs a source-side pull-to-
scratch prologue (via the copy engine's `pull_path_to_local` seam) before the ordinary local ingest, orthogonal to the
archive parent's local-vs-remote handling. Move deletes the remote originals after the durable commit. Canonical docs:
`write_operations/DETAILS.md` § "Archive edits" (source-side pull bullet).

## 6. Remote-backed archive live refresh (UX, niche)

A LOCAL archive listing live-refreshes when the backing file changes on disk (real `notify` watch on the `.zip`). A
REMOTE archive doesn't (no watch transport over SMB / MTP). Options: poll `get_metadata` (size + mtime) on a
visible-pane cadence, or accept manual refresh as the contract. Decide deliberately; today's behavior is stale-until-
refresh with no indicator.

## 7. Remote edit temp reaping (data hygiene, small)

A remote zip edit uploads under a temp name and swaps. A crash/cancel exactly between upload and swap can leave the temp
under its remote name; local leftover temps get reaped at the next edit of the same archive, remote ones don't
(documented non-reap). A next-edit-of-same-remote-archive reap (list siblings, delete `*.cmdr-tmp-*` older than a
threshold) closes it. Small, bounded, and the crash window already never loses NEW data.

## 8. Move-out per-entry delete refinement (edge-case correctness)

Move OUT of a zip is all-or-nothing: any skip/error/cancel during extract deletes nothing from the archive (correct, no
data loss, but a partially-moved tree stays fully inside the zip). A per-entry refinement — delete exactly the entries
whose extraction proved durable — would make partial moves converge instead of restarting. Guarded by the same
delete-only-after-durable-commit invariant; the batch shape was chosen to dodge a partial-merge-skip hazard, so revisit
that analysis first (write_operations `DETAILS.md` § archive edits).

## 9. M6: MTP in-place editing (stretch, cross-repo)

In-place remote editing for MTP devices (today: pull-edit-upload-swap, which is correct but O(archive) over USB both
ways). Touches `mtp-rs` (first-party). Stretch from the original plan; only worth it if MTP zip editing sees real use.
Notes: `../archive-browsing-plan.md` § M6.

## 10. Dev-side debt (warns and tight margins)

- `file-length` growth warns needing trim-or-consent: `transfer/volume_copy_tests.rs` (2461 lines, allowlist 2102),
  `app.css` (1579, allowlist 1202), `indexing/manager.rs` (1289, allowlist 1147), `listing/caching_test.rs` (1304,
  allowlist 1168), plus ~14 unlisted files newly over 800. Split/trim where it's an architecture win; otherwise ask
  David for allowlist consent explicitly.
- Four archive-area `CLAUDE.md`s sit at 595–599 words against the 600 ceiling (volume/, backends/archive/,
  write_operations/, pane/). The next few added sentences re-trip the warn; the honest fix then is a folder split
  (backends/archive/ is the candidate: 20+ files spanning read core, formats, and mutation), not another squeeze.
- `smb.spec.ts` `recreateFixturesAndSettle()` uses a rationale-documented `sleep(1000)`; likely convertible to
  `flushFileWatcher()` + a `fileExistsInFocusedPane` poll, but verifying needs the SMB Docker E2E lane.
- One E2E duration warn: the cancel-paste spec runs ~4.4 s (suite target: well under a second per test).

## 11. Awaiting David (no agent action)

- QA nits from his visual pass (Enter menu, Archives settings, queue rows, delete confirm, damaged-archive banner).
- English copy review, incl. the two raw Rust backstop strings.
- Translation terminology flags: zh/vi compressed-file overrides, nl "App-pakketten", es "cifrado", hu "Rákérdezés", sv
  "Appaket", vi "bản mới".
- The `quick-xml` cargo-audit advisory is Renovate's to close (transitive dep, advisory published 2026-07).
