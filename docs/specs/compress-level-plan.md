# Compress: compression level + estimated size plan

Status: planning. Worktree `.claude/worktrees/compress-level`, branch `david/compress-level` off local `main` at
`68c9053d5` (the Compress feature is fully shipped). Every path, symbol, and signature below was verified against the
live tree on 2026-07-09; re-verify before editing, but don't re-derive.

Extends the shipped Compress feature (see [`compress-feature-plan.md`](compress-feature-plan.md) for the base
architecture — the Transfer dialog's third mode, `compress_files` IPC, `compress_start` seeding a 22-byte empty zip then
`route_archive_copy_into`). Two independent asks:

- **Feature 1 (decided, build it): a compression-level slider.** In the Compress dialog and in Settings, one persisted
  setting, applied to the zip the operation writes.
- **Feature 2 (spike-gated, prove it first): an estimated result size** shown live-ish in the dialog. Ships only if a
  measurement spike clears a reliability bar; otherwise it ships as nothing and the spike notes go to `docs/notes/`.

Feature 1 lands first (M1–M5). Feature 2 (M6 spike → M7 wiring, conditional) depends on it, because the estimate must
reflect the chosen level.

---

## Goal

**Feature 1.** A single "compression level" setting (deflate 1–9, default 6) that:

- Renders as a slider in Settings under **Behavior › Archives** (the section already exists), with "Faster"/"Smaller"
  end labels.
- Renders as the same slider inside the Compress dialog; moving it there persists the same setting immediately (dialog
  and Settings are one value, no separate dialog-local state).
- Threads to the archive mutator so the zip Compress writes uses that level. Because the mutator is shared, the setting
  also governs regular copy/move **into** an existing archive (one uniform "compression level" concept for all
  user-driven zip writes). Internal zips (crash/error-report bundles) keep their own fixed level and are out of scope.

**Feature 2.** A live estimate of the compressed output size in the Compress dialog (e.g. an explicitly approximate "~
42 MB" beside the scanned byte total), driven off the existing deep byte-scan by cheap deflate sampling, hard-capped in
bytes and time, cancellable with the scan, never blocking or destabilizing the scan/conflict flow. Ships only if the
spike proves it reliable and cheap enough.

---

## Verified findings the plan relies on (do not re-derive)

Frontend paths under `apps/desktop/src/`, backend under `apps/desktop/src-tauri/src/`.

### The zip crate's level API (`zip` v8.6.0, vendored, flate2 backend — verified in the crate source)

Cmdr builds `zip` with `default-features = false, features = ["deflate", "aes-crypto"]`
(`apps/desktop/src-tauri/Cargo.toml:121`), so the deflate backend is **flate2** (zopfli is NOT compiled in). Therefore:

1. `FileOptions::compression_level(self, level: Option<i64>)` (`zip-8.6.0/src/write.rs:548`). `SimpleFileOptions` is the
   `FileOptions<'static, ()>` alias. The level type is **`Option<i64>`**.
2. For Deflated, the accepted range is **1..=9** (`deflate_compression_level_range()`, `write.rs:2332-2361`:
   `flate2::Compression::fast().level()` = 1 .. `flate2::Compression::best().level()` = 9).
3. An out-of-range level is **not clamped** — `validate_value_in_range` (`write.rs:2377-2387`) returns `None`, which
   becomes `Err(ZipError::UnsupportedArchive("Unsupported compression level"))` at the FIRST entry write (in
   `get_compressor`, `write.rs:2076-2080`). Cmdr surfaces this as `MutationError::Zip`. **So the level MUST be
   constrained to 1..=9 before it reaches the mutator** — defend it in Rust (clamp), not only via the UI constraint.
4. `None` (the default) maps to deflate level **6** (`flate2::Compression::default().level()`, `write.rs:2061-2065`). So
   "default = 6" and "unset" are the same output; the setting's default of 6 is the crate's current behavior.

### The single production zip-write site that must follow the setting

There is exactly **one** production `FileOptions` site that should follow the user setting: the archive-edit mutator's
`add_entry_options`, in `file_system/volume/backends/archive/mutation/mutator.rs`:

- `fn add_entry_options(add: &AddEntry) -> SimpleFileOptions` (mutator.rs:375-388) builds
  `SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)` with **no** `.compression_level` (so it
  defaults to 6 today). Used at `start_file(...)` (mutator.rs:275) for **new** entries only. Retained/unchanged entries
  are raw-copied byte-for-byte (`raw_copy_file_rename`, mutator.rs:254) and are NOT recompressed — the level applies
  only to newly added data. This is the thread-in point.

Everything else that builds `FileOptions` is either internal or test-only and stays fixed:

- `error_reporter/bundle_builder.rs` (lines 200-202, 262-264) and `error_reporter/bundle_capper.rs` (95-96, 143-144)
  deliberately use fixed level 1 for crash/error bundles — **out of scope, do not touch.**
- The file viewer has **no** production zip-write path (its only `FileOptions` use is a `#[cfg(test)]` fixture in
  `commands/file_viewer.rs`). All other hits across the tree are `#[cfg(test)]`.

### The call chain the level threads through

`compress_start` and the copy/move-into-archive routing both funnel through **`route_archive_copy_into`**
(`write_operations/archive_edit/copy_into.rs:62`). It builds a `Changeset` of `AddEntry` items (via `plan_copy_into` →
`adds.push(AddEntry { ... })` around copy_into.rs:452) and applies it with `mutator::apply(working, &changeset, hooks)`
(invoked through the managed-edit engine; see `archive_edit_start` in `driver.rs:105` and `mutator::apply` at
`driver.rs:174`). `add_entry_options` is called inside `mutator::apply` per add. So the level must reach
`mutator::apply` — cleanest via a field on the `Changeset` (one level per edit), read by `add_entry_options`.

`AddEntry`, `AddSource`, `Changeset`, `MutationError` live in `backends/archive/mutation/mutator.rs`;
`route_archive_copy_into` and `compress_start` are re-exported through `write_operations/mod.rs` and
`file_system/mod.rs`.

### The write-operation config carrying the level from the frontend

`compress_files` and `copy_between_volumes` (both in `commands/file_system/volume_copy.rs`) take
`config: Option<VolumeCopyConfig>`. `VolumeCopyConfig` (`write_operations/types.rs:649-680`) currently holds
`progress_interval_ms`, `conflict_resolution`, `max_conflicts_to_show`, `preview_id`, `pre_known_conflicts`. It's the
natural carrier for `compression_level` — no new command params, and it's `#[serde(default)]`-friendly so old callers
stay valid. (`WriteOperationConfig`, types.rs:507-531, is the legacy local-only config; the archive routing rides
`VolumeCopyConfig`, so add the field there. Add to `WriteOperationConfig` too only if a threading gap shows up — verify
during M1, don't add speculatively.)

### The settings machinery (for the slider)

- **The setting is FRONTEND-OWNED**, like the existing `behavior.archiveEnterBehavior`: read on the frontend at
  operation time and passed to the backend via the operation config. The backend never reads it independently, so there
  is **NO** `commands/settings.rs` command, **NO** `settings/loader.rs` field, and **NO** `settings-applier` entry.
  (That whole live-apply chain is only for settings Rust reads on its own; this one isn't.)
- **Registry**: `settings/settings-registry.ts`. Closest analog for a stepped 1–9 slider is `appearance.textSize` (lines
  281-296: `type: 'number'`, `component: 'slider'`, `constraints: { min, max, step, sliderStops }`, `default`). The
  existing Archives entry is `behavior.archiveEnterBehavior` (registry ~572-585; `section: ['Behavior', 'Archives']`).
- **Typed value map**: `settings/types.ts` — add the key to `interface SettingsValues` (a missing key is a
  `svelte-check` error). Additive key; do **not** bump `SCHEMA_VERSION`.
- **Slider component**: `settings/components/SettingSlider.svelte` — self-contained: reads `getSetting(id)`, writes
  `setSetting(id, ...)` on change, subscribes via `onSpecificSettingChange` for external resets, double-click thumb
  resets to default, snaps to `sliderStops`. It bundles an Ark UI `Slider` + a paired `NumberInput` and tick marks.
  Because it reads and writes the setting purely by `id`, dropping
  `<SettingSlider id="behavior.archiveCompressionLevel" />` into the Compress dialog gives the dialog and Settings the
  same value for free.
- **Section component**: `settings/sections/ArchivesSection.svelte` (a custom, hand-rendered section using
  `SettingsSection` + `SectionCard`). It currently renders per-format Enter-behavior `ToggleGroup`s; add the
  compression-level row here.
- **MCP `set_setting`** is fully generic (`mcp/executor/async_tools.rs::execute_set_setting`, round-trips `{id, value}`
  to the FE registry) — a new registry key works via MCP automatically, no MCP changes.

### The scan machinery (for the estimate)

- The deep byte scan is **backend-driven (Rust), streaming progress over Tauri events**;
  `transfer/transfer-scan-state.svelte.ts` (`createTransferScanState`, line 65) is a thin listener that accumulates
  aggregate counters (`filesFound`, `dirsFound`, `bytesFound`, `dedupBytesFound`) from event payloads. It never sees
  individual files and never reads contents.
- Local walk: `start_scan_preview` (`write_operations/scan_preview.rs:37`) → `run_scan_preview` (:112) →
  `walk_dir_recursive` (`write_operations/scan.rs:64`). The per-file branch is `scan.rs:89-105` — **stat only today**
  (`symlink_metadata` + `metadata.len()`), no file open. This is the clean seam to add byte-sampling: it already holds
  `path` and size and runs once per file on the single existing walk.
- **Two walk paths, and only the local one is a cheap sampling seam.** The volume/oracle path
  (`run_volume_scan_preview`, scan_preview.rs:246; `run_oracle_aware_batch_scan`, :371;
  `walk_cached_entries`/`scan_subtree_with_oracle` in scan.rs) sizes from cached listings with NO file open — sampling
  there means real MTP/SMB reads or defeats the oracle's zero-I/O short-circuit. So content-sampling is local-FS-only;
  remote/oracle-cached files fall back to an extension-based ratio (or the estimate is suppressed for them — the spike
  decides).
- **Cancellation is inherited for free**: the walk checks `is_cancelled` before each file (scan.rs:77, wired to
  `state.cancelled` AtomicBool). The TS factory resets estimate state on `cancelPreview`/`freeAndCleanup` (called from
  `TransferDialog.svelte` `onDestroy`/`handleCancel`), so any estimate state cancels with the scan.
- **Event payloads**: `ScanPreviewProgressEvent` / `ScanPreviewCompleteEvent` (`write_operations/types.rs`, emitted at
  scan_preview.rs:148 and :217). An estimated-compressed-bytes field is added here (Option, default None), consumed by
  the TS listeners (transfer-scan-state.svelte.ts:107-124).
- **Display**: `TransferDialog.svelte` renders the scanned total at line 560 (`<Size bytes={bytesFound} />` inside the
  `.scan-stats` block, lines 557-591). `Size` is `$lib/ui/Size.svelte`. The estimate line goes beside it (a new
  `.scan-stat`, or a sibling line below `.scan-stats` like the `hardlink-note` `<p>` at 597-601).
- **No existing** compression-ratio estimation, byte-sampling, or incompressible-extension table anywhere — Feature 2
  builds it from scratch. `flate2 = "1"` is a direct dep of `src-tauri` (Cargo.toml:148), usable for sampling.

### Check-lane names (verified — the shipped plan used some wrong ones)

`rust-tests`, `svelte-tests`, `eslint-typecheck-ts`, `clippy`, `file-length`. i18n lanes are `i18n-parity`, `i18n-icu`,
`i18n-plural`, `i18n-stale`, `i18n-coverage` (there are `desktop-`-prefixed aliases too; prefer the unprefixed names).

---

## Architecture at a glance

```
Feature 1 — compression level
  Settings › Behavior › Archives  ─┐
                                   ├─ <SettingSlider id="behavior.archiveCompressionLevel">  (getSetting/setSetting)
  Compress dialog (same slider)   ─┘            │  persisted to settings.json (FE-owned)
                                                ▼
  compress / copy / move config builder reads getSetting → config.compressionLevel
                                                │  compressFiles(...config) / copyBetweenVolumes(...config)
                                                ▼
  [Rust] compress_files / copy_between_volumes  ──►  VolumeCopyConfig.compression_level (clamp 1..=9)
                                                │
                                                ▼
  route_archive_copy_into(..., compression_level)  ──►  Changeset.compression_level
                                                │
                                                ▼
  mutator::apply ──► add_entry_options(add, level) ──►  .compression_level(Some(level))  on NEW entries

Feature 2 — estimated size (spike-gated)
  deep byte-scan walk (scan.rs walk_dir_recursive, local FS)
        │  per file > threshold, under a global byte+time budget:
        │    open, read a small window, deflate at reference level, measure ratio
        │    incompressible ext → ratio ~1.0 (table);  remote/oracle → ext-ratio fallback
        ▼
  accumulate estimated_compressed_bytes ──► ScanPreview{Progress,Complete}Event
        │
        ▼
  transfer-scan-state (estimatedBytes)  ──►  TransferDialog "~ 42 MB" (scaled to the selected level via spike curve)
```

---

## Milestones

Sequential, single-agent, each ends green (`pnpm check --fast` minimum plus the scoped lanes named) and is committable.
Backend logic is real red→green TDD.

### M1 — Backend: thread the compression level through the archive mutator (Rust, TDD)

The core of Feature 1. Purely backend; no UI yet. Prove the level actually changes the output before wiring any UI.

- **`VolumeCopyConfig`** (`write_operations/types.rs:649`): add `#[serde(default)] pub compression_level: Option<i64>`
  (None = crate default 6). Update the `Default` impl and the `From<&WriteOperationConfig>` impl (types.rs:682) — if you
  add the field to `WriteOperationConfig` too, carry it there; if not, map to `None`. Regenerate specta bindings (the
  binding-gen step; the check lane flags staleness) so `VolumeCopyConfig` on the TS side gains the field.
- **`compress_start`** (`write_operations/archive_edit/compress.rs`): add a `compression_level: Option<i64>` param, pass
  it to `route_archive_copy_into`.
- **`route_archive_copy_into`** (`archive_edit/copy_into.rs:62`): add a `compression_level: Option<i64>` param; store it
  on the `Changeset` it builds. Update all callers (compress_start, and the copy/move-into-archive routing in
  `commands/file_system/volume_copy.rs` — `copy_between_volumes`/`move_between_volumes` extract
  `config.compression_level`). A caller that has no opinion passes `None`.
- **`Changeset`** (`backends/archive/mutation/mutator.rs`): add `compression_level: Option<i64>`.
- **`add_entry_options`** (mutator.rs:375): take the level (either as a param `add_entry_options(add, level)` or read it
  off the changeset) and set `.compression_level(Some(clamped))`. **Clamp to 1..=9 in Rust**
  (`level.map(|l| l.clamp(1, 9))`) so an out-of-range value can never reach `get_compressor` and hard-fail the edit (see
  Verified findings). Keep `CompressionMethod::Deflated`.
- **TDD (red first, for the right reason):**
  - Unit-test `add_entry_options`: level `Some(1)` and `Some(9)` produce `FileOptions` with the expected
    `get_compression_level()`; `None` yields `None` (crate default); out-of-range (`Some(0)`, `Some(42)`) clamps into
    1..=9. Write assertions before the clamp exists → RED → implement.
  - Integration test (mirror the existing `compress_tests.rs` / `copy_into_tests.rs` harness in `archive_edit/`):
    compress the SAME compressible payload (a few KB of repetitive text) at level 1 and level 9 into two zips; assert
    the level-9 zip's stored entry size is `<=` the level-1 one, and both round-trip to the original contents. (Use a
    genuinely compressible payload so the sizes actually differ.) A no-op change (level 6 vs default None) must produce
    byte-identical entry sizes — asserts the default is faithful.
  - Do NOT pile these into `transfer/volume_copy_tests.rs` (already an allowlisted growth warn, 2461/2102). New tests
    live in `archive_edit/compress_tests.rs` (or a sibling) next to the code.
- **Docs:** add a "Compression level" note to the archive-edit `DETAILS.md` (the mutator sets the level on NEW entries
  from the op config; retained entries are raw-copied and unaffected; clamp rationale). One `CLAUDE.md` guardrail line
  only if omitting the clamp can silently break an edit — it can ("clamp the level to 1..=9 before `add_entry_options`;
  the zip crate hard-errors on out-of-range, not clamps"). Watch the archive-area `CLAUDE.md` word budgets (several near
  600/600 — condense, don't append).
- **Checks:** `pnpm check clippy rust-tests -q` scoped, then `pnpm check --fast`.

### M2 — Frontend: the setting + Settings row + config threading

- **Registry** (`settings/settings-registry.ts`): add
  ```
  { id: 'behavior.archiveCompressionLevel', section: ['Behavior', 'Archives'],
    labelKey: 'settings.archives.compressionLevel.label',
    descriptionKey: 'settings.archives.compressionLevel.description',
    keywords: ['compression', 'level', 'zip', 'deflate', 'archive', 'size', 'faster', 'smaller'],
    type: 'number', default: 6, component: 'slider',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1,2,3,4,5,6,7,8,9] } }
  ```
  (model on `appearance.textSize`, lines 281-296).
- **Types** (`settings/types.ts`): add `'behavior.archiveCompressionLevel': number` to `interface SettingsValues`. No
  `SCHEMA_VERSION` bump.
- **Section render** (`settings/sections/ArchivesSection.svelte`): add a `SectionCard` (or a row in a fitting existing
  card) hand-rendering `<SettingRow>` wrapping `<SettingSlider id="behavior.archiveCompressionLevel" />`, with "Faster"
  (left) and "Smaller" (right) end labels, guarded by `shouldShow(...)` / `anyVisible(...)` like the existing cards.
  (Check `SettingRow` is the row wrapper the section uses; ArchivesSection currently hand-rolls `.archive-row` — follow
  whichever keeps the card visually consistent.)
- **Config threading (FE → backend):** where the transfer operation config is built for compress AND copy/move (the
  config object passed to `compressFiles` / `copyBetweenVolumes` / `moveBetweenVolumes` — trace from
  `transfer-progress-state.svelte.ts::createTransferProgressState`, which the shipped compress plan identified as the
  dispatch point), populate `compressionLevel: getSetting('behavior.archiveCompressionLevel')`. Read the setting once at
  dispatch time (not reactively). This makes the level uniform across compress and copy/move-into-archive; for
  non-archive copies the backend simply ignores it.
- **English strings** (`intl/messages/en/`): `settings.archives.compressionLevel.label` + `.description`, and the
  "Faster"/"Smaller" end-label keys (reuse in M3). Each with a `@key.description` meeting the i18n bar. Follow the style
  guide (sentence case, active voice, no "just/simple"). Run `pnpm intl:keys`.
- **Tests:** a `svelte-tests` unit/interaction test that the Archives section renders the slider at the current setting
  and that changing it persists via `setSetting` (mock the store). A `settings.spec.ts` (Playwright) touch only if the
  section-order assertion needs the new row — otherwise leave E2E to M5.
- **Checks:** `pnpm check eslint-typecheck-ts svelte-tests -q` scoped, then `pnpm check --fast`.

### M3 — Frontend: the compression-level slider inside the Compress dialog

`TransferDialog.svelte` is **pinned at 976 lines** in the file-length allowlist and sits over the 800 warn line, so
**net growth must be ≤ 0**. Push the slider into a small helper/child component and keep the dialog markup minimal;
never bump the allowlist.

- **Show the slider only in compress mode** (`activeOperationType === 'compress'`), near the scan/summary area.
  Recommended: a tiny child component `transfer/CompressLevelControl.svelte` (well under 800) that renders
  `<SettingSlider id="behavior.archiveCompressionLevel" />` framed by "Faster"/"Smaller" labels and a short caption. The
  dialog imports and conditionally renders it — a handful of lines, ideally offset by trimming elsewhere so
  TransferDialog nets down. Because `SettingSlider` reads/writes the setting by id, moving it here persists the value
  immediately and Settings reflects it live (and vice versa) with zero extra wiring — this satisfies David's "value
  saved to config" ask directly.
- **No dialog-local level state.** Do not copy the value into a dialog field; the setting store is the single source.
  The confirm path already reads the setting at dispatch time (M2), so whatever the slider last persisted is what the
  operation uses.
- **File-length:** run `pnpm check file-length`; confirm TransferDialog does not grow (ratchet it DOWN if you trim). If
  `CompressLevelControl.svelte` approaches 800, it won't — it's a thin wrapper. Never allowlist a new file.
- **Tests:** a `svelte-tests` test that switching the dialog to Compress shows the slider and that moving it calls
  `setSetting('behavior.archiveCompressionLevel', ...)`. Add to a NEW spec file, not `TransferDialog.test.ts` (852,
  warn-only — don't grow it).
- **Checks:** `pnpm check eslint-typecheck-ts svelte-tests file-length -q` scoped, then `pnpm check --fast`.

### M4 — i18n: translate the Feature 1 strings to every locale

Same process as the shipped compress plan's M6. Strings are few (level label + description, "Faster", "Smaller", the
dialog caption).

- Locales: 10 non-English (`de`, `es`, `fr`, `hu`, `nl`, `pt`, `sv`, `vi`, `zh`) plus `en` — 11 dirs under
  `src/lib/intl/messages/`.
- Follow `docs/guides/i18n-translation.md` § "New feature → add strings and translate to ALL languages":
  1. Confirm every new `en` key has a `@key.description` meeting the bar (surface, trigger, meaning of
     "Faster"/"Smaller" as compression tradeoff ends, not speed/size of the UI).
  2. `node apps/desktop/scripts/sync-locale-keys.js` to propagate skeletons with correct `sourceHash`.
  3. Per locale: read `docs/i18n/<tag>/style.md`, mine the reference pile at the **absolute main-clone path**
     `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/` (NOT the worktree — the pile isn't copied in; the worktree path
     looks empty, the documented trap). The two-pane pair (Total Commander, Double Commander) is the lineage match for
     archive-compression terms. Record any `tentative` term in the per-language glossary.
  4. `pnpm check i18n-parity i18n-icu i18n-plural i18n-stale i18n-coverage`.
- **Checks:** the i18n lanes above, then `pnpm check --fast`.

### M5 — Feature 1: docs sync + E2E + verify

- **Docs sync** (colocated, per `.claude/rules/docs.md`): the archive-edit `DETAILS.md` (the M1 note — single-sourced
  there), the settings/Archives-section C+D if it lists the section's rows, the `transfer/` C+D (the compress-mode
  slider). Single-source: the level mechanism lives in ONE archive-edit `DETAILS.md`; everything else points to it.
- **E2E** (extend `apps/desktop/test/e2e-playwright/compress-basic.spec.ts` only if cheap — read
  `test/e2e-playwright/CLAUDE.md` first): set the level to 1, compress a compressible fixture; set it to 9, compress the
  same; assert the level-9 output is not larger (byte-level, by navigating into the archive or statting it). Skip if the
  E2E harness can't observe entry sizes cheaply — the M1 Rust integration test is the real coverage; note that in the
  spec.
- **Verify end to end** (drive the real app per the `verify` skill): move the Settings slider, compress a folder,
  confirm the setting persists across a dialog reopen and an app restart, and that the produced zip opens in Finder /
  Archive Utility and Cmdr's own archive browser. Confirm the dialog slider and the Settings slider stay in sync.
- **Checks:** `pnpm check` (add `--include-slow` for the E2E lane). Sanctioned pre-existing red: the `quick-xml`
  cargo-audit advisory (Renovate's to close).

### M6 — Feature 2: estimator spike + kill-criterion gate (measure before building UI)

**This milestone decides whether Feature 2 ships. Build the estimator, measure it on real mixes, and stop at the gate.**
No dialog UI in M6.

> **M6 DONE (2026-07-09) — VERDICT: GO for the local sampling estimator; SUPPRESS the estimate on remote (SMB/MTP)
> sources.** Full evidence, decision table, and recommended M7 parameters:
> [`docs/notes/compress-size-estimate-spike.md`](../notes/compress-size-estimate-spike.md). Summary: on five real mixes
> the local sampling estimator clears both bars with margin (overall median absolute error 1.3%, worst realistic mix
> 6.9%); only the deliberately adversarial synthetic mix exceeds 30% (37%, in the safe overestimate direction).
> Extension-only clears the realistic-mix bars too but has an unbounded silent failure (833% on one mistyped file) with
> no sampling safety net, so remote stays suppressed per the lead default. Recommended M7 params: 32 KiB head window, 8
> MiB byte budget, 4 KiB tiny threshold (running-average ratio), the incompressible-extension shortcut table, and the
> measured per-class level-scaling curve applied arithmetically (no re-sampling per slider tick). Bounded added cost
> ~105 ms worst case, near-zero for media-heavy folders; run the sample-deflate off the walk thread. **M7 is GO.** Side
> finding for Feature 1: with `flate2`/`miniz_oxide`, levels 6–9 differ by < 0.5% (the "Smaller" half of the slider is
> nearly inert; all real reduction is at levels 1–4).

- **Build the Rust estimator behind the scan (local FS path only):** in `walk_dir_recursive`'s per-file branch
  (scan.rs:89-105), behind a `sample_for_estimate: bool` flag threaded from `start_scan_preview` (only set for compress
  scans), for each file:
  - Known-incompressible extension (jpg/jpeg/png/gif/webp/heic/mp4/mov/mkv/avi/mp3/aac/flac/zip/gz/xz/7z/bz2/zst/rar/…)
    → ratio ≈ 1.0 from a static table, no read.
  - Tiny file (< a threshold, e.g. 4 KiB) → count as-is (ratio ≈ 1.0; zip per-entry overhead makes tiny files ~neutral).
  - Otherwise, under a **global byte budget** (hard cap, e.g. 4 MiB sampled per scan) and a **global time budget** (e.g.
    stop sampling after N ms), open the file, read a window (e.g. 64 KiB from the head; optionally a second window mid),
    deflate at a **reference level** (6) with `flate2`, compute the window ratio, extrapolate to the file's full size.
    Once a budget is exhausted, remaining files use the extension table / a global running average ratio.
  - Accumulate `estimated_compressed_bytes` alongside the existing `total_bytes`/`dedup_bytes` (a third counter, not a
    replacement). Remote/oracle-cached files (the non-local walk paths) get extension-ratio only — no reads.
- **Level scaling:** sample once at the reference level; derive per-level multipliers (level→ratio-vs-ref) as a small
  fixed curve **measured in this spike**, so moving the dialog slider re-scales the shown estimate without re-sampling.
  The spike must quantify the extra error this scaling introduces.
- **Measurement harness (the actual deliverable of M6):** a repeatable measurement (a Rust bench/test or a small script
  under `docs/notes/`) that, for several realistic mixes, compares the estimate to the ACTUAL zip size produced by the
  real mutator at several levels:
  1. a source-code tree (highly compressible),
  2. a photo folder (JPEGs — near-incompressible),
  3. mixed office docs / PDFs (medium),
  4. an already-compressed pile (zips/videos — incompressible),
  5. a large mixed real folder (whatever's handy on the machine). Record: median and worst-case absolute % error per mix
     and overall; the scan wall-time overhead added by sampling (with vs without); the sampled-byte totals hit against
     the cap.
- **Kill criterion (gate — put the measured numbers in the plan/notes and decide):**
  - **Accuracy bar:** median absolute error ≤ **15%** overall, worst-mix median ≤ **30%**, with the estimate always
    shown as explicitly approximate ("~") and styled as an estimate.
  - **Resource bar:** sampling adds ≤ **20%** to scan wall-time on the test mixes (or ≤ ~300 ms absolute on a large
    mix), and never exceeds the byte/time budgets.
  - **If BOTH bars pass →** proceed to M7. **If either fails** (and can't be met by tuning window size / budget /
    thresholds within this milestone), **Feature 2 ships as nothing**: write the estimator + measurements up in
    `docs/notes/` (why it didn't clear the bar, the numbers, what would change the verdict), revert or gate-off any
    scan-path changes so the scan stays pure-stat, and stop. Feature 1 is unaffected.
- **Checks:** `pnpm check clippy rust-tests -q` scoped (the estimator + measurement), then `pnpm check --fast`.

### M7 — Feature 2: wire the estimate into the dialog (CONDITIONAL on M6 passing)

Only if M6 cleared the gate. Otherwise skip entirely.

- **Event payload:** add `estimated_compressed_bytes: Option<u64>` to `ScanPreviewProgressEvent` /
  `ScanPreviewCompleteEvent` (`write_operations/types.rs`), populated from M6's counter; regenerate bindings. `None`
  when unavailable (non-compress scan, or estimate suppressed).
- **Scan state:** add `estimatedBytes` to `transfer-scan-state.svelte.ts` (updated in the progress/complete listeners,
  reset in `cancelPreview`/`freeAndCleanup` — so it cancels with the scan). Expose via a getter.
- **Dialog UI:** in `TransferDialog.svelte`, in compress mode only, render the estimate beside the scanned total (near
  line 560) as `~ <Size bytes={estimatedBytes} />`, styled to read as an estimate. States: while scanning show a
  loading/updating affordance; if unavailable (remote sources / suppressed / null), show nothing or a subtle "estimate
  unavailable" — do NOT show a wrong number. Moving the level slider re-scales the shown estimate via the M6 curve (no
  re-scan). **It must never delay or block the scan/conflict/confirm flow — it's additive and best-effort.** Keep
  TransferDialog net growth ≤ 0 (push formatting/scaling into a helper).
- **i18n:** any new string (e.g. "estimate unavailable", the "~" caption if worded) across all 11 locales via the M4
  process.
- **E2E:** extend `compress-basic.spec.ts` to assert the estimate line appears for a compressible local fixture; keep it
  cheap. Cancel case already covered by the base compress spec.
- **Docs:** document the estimator (sampling seam, budgets, extension table, level-scaling curve, remote fallback) in
  the scan/write-operations `DETAILS.md`; link the M6 measurement note in `docs/notes/`. Guardrail line only if a future
  edit could silently break the budget/pure-stat-for-remote invariant.
- **Verify:** drive the app — estimate a compressible folder and an incompressible one, move the slider, confirm the
  number moves sensibly and the scan never stalls.
- **Checks:** `pnpm check` (with `--include-slow` for E2E).

---

## File-length allowlist risk spots

- **`TransferDialog.svelte` (pinned 976, over the 800 warn line).** M3 and M7 both touch it; each must net ≤ 0. Put the
  slider in `CompressLevelControl.svelte` and the estimate formatting/scaling in a helper. Run `pnpm check file-length`
  after each; ratchet the pin DOWN if you trim. **Never bump the allowlist for this file.**
- **`TransferDialog.test.ts` (852, warn-only).** New dialog tests go in separate spec files, not here.
- **`transfer/volume_copy_tests.rs` (2461/2102, allowlisted growth warn).** Don't add compress-level tests here — they
  live in `archive_edit/compress_tests.rs` next to the code.
- **New helper files** (`CompressLevelControl.svelte`, any estimator module) stay under 800; split if one would cross.
- **Archive-area `CLAUDE.md` word budgets** (several near 600/600). M1/M5/M7 doc edits condense, don't append; a folder
  split beats another squeeze. Warn-only — surface to David rather than silence.

---

## Decided questions (recommendations for the genuinely open ones)

1. **The setting is frontend-owned, threaded via the operation config — NOT a backend-read setting.** Like
   `behavior.archiveEnterBehavior`: the FE reads it at dispatch and passes `compression_level` in `VolumeCopyConfig`. So
   no `commands/settings.rs` command, no `loader.rs` field, no applier entry. Rationale: the level is only ever needed
   at operation time, and threading it explicitly keeps the backend stateless and the value testable. (If a future
   feature needs Rust to read the level without an operation — e.g. a non-interactive MCP compress that bypasses the
   config — revisit and add a loader field then.)
2. **One uniform "compression level" for all user-driven zip writes** (compress AND copy/move-into-archive), because the
   mutator is shared and threading it to the copy path is nearly free (one FE config field). Internal zips
   (crash/error-report bundles) keep their fixed level 1 and are out of scope — they're diagnostic artifacts, not user
   content. The file viewer has no write path. This is deliberate; don't scope the setting to compress-only.
3. **Slider semantics: raw 1–9, default 6, "Faster"/"Smaller" end labels** (not named presets). Matches the crate's
   actual deflate range and the house `appearance.textSize` stepped-slider pattern; 6 is the crate default so the
   setting's default is a no-op vs today. The number input from `SettingSlider` shows the exact level for power users.
4. **Reuse `SettingSlider` in the dialog rather than a bespoke control.** It reads/writes the setting by `id`, so the
   dialog and Settings share state with zero extra wiring, satisfying "changing it in the dialog saves to config." Frame
   it with "Faster"/"Smaller" labels in a thin `CompressLevelControl.svelte`. If `SettingSlider`'s paired
   NumberInput/ticks look too heavy in the dialog, the fallback is a minimal slider in that wrapper that still calls
   `setSetting` — decide visually in M3, but try the reuse first (elegance + single-source).
5. **Clamp the level to 1..=9 in Rust**, at `add_entry_options` (or just before). The zip crate hard-errors on an
   out-of-range level (it does NOT clamp), which would fail the whole edit at the first entry. The UI constraint is not
   enough — a bad config value, an MCP `set_setting` with a wild number, or a future caller must not be able to break an
   edit.
6. **MCP: the compress tool gains no new parameter.** The level is a persisted setting, so MCP `compress` uses whatever
   the setting is; MCP `set_setting` already sets it generically. Rationale: keeps the MCP surface minimal and matches
   "setting-driven." (If an MCP caller later needs a per-call override, add an optional param then and thread it into
   the same config field.)
7. **Feature 2 is genuinely spike-gated and may ship as nothing.** The estimator is only cheap and clean on the local-FS
   walk. M6 measures real accuracy and cost against explicit bars (median ≤ 15%, worst-mix ≤ 30%, +time ≤ 20%); a miss
   means the notes go to `docs/notes/` and no UI ships. Do not build M7 UI before M6 passes. **Remote sources (lead
   decision): SUPPRESS the estimate by default** — no sampling over SMB/MTP (it would do real remote reads and defeat
   the scan oracle's budget), and no extension-ratio guess UNLESS the M6 data shows extension ratios alone clear the
   SAME bars on realistic mixes. An absent estimate is honest; a wide guess styled like a measurement is not.
8. **The estimate reflects the selected level via a measured scaling curve, sampled once at level 6** — not re-sampled
   per slider tick (David's "not too wasteful" constraint). M6 quantifies the scaling error and folds it into the
   accuracy bar.

---

## Definition of done

**Feature 1:** a "compression level" slider (1–9, default 6, Faster/Smaller labels) lives in Settings › Behavior ›
Archives and inside the Compress dialog, both driving one persisted `behavior.archiveCompressionLevel` setting; the
chosen level applies to the zip Compress writes and to copy/move-into-archive; the level is clamped 1..=9 in Rust; the
mutator threading is red→green tested (level 1 vs 9 differ; default is a no-op); strings translated across 11 locales;
TransferDialog nets no growth (no allowlist bump); colocated C+D current and single-sourced. **Feature 2:** either
shipped — a live, explicitly-approximate estimate in the dialog that cleared the M6 accuracy and resource bars, cancels
with the scan, and never destabilizes the scan/conflict flow — or explicitly NOT shipped, with the estimator and its
measurements written up in `docs/notes/` and the scan left pure-stat. Full `pnpm check` green (bar the sanctioned
`quick-xml` red). Self-reviewed solid AND elegant.
