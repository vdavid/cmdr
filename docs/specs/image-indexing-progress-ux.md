# Image-indexing progress + settings UX

Four changes in the image-indexing area, per David. Decisions are LOCKED (see each section). Smart-backend/thin-frontend,
house UI primitives, every user-facing string i18n'd (translator pass follows for all 10 locales), `pnpm check`.

## 1. Honesty fix: a distinct "indexing" state (no false "pending" flash)

Today the per-file badge reads `pending` ("waiting to be indexed") for any covered image with no `media_status` row —
including a file whose row was transiently deleted (a real move/rename GC) while a pass re-enriches it. That misreads as
"never indexed."

- Add `FileIndexState::Indexing` (serialized `"indexing"`) to the enum in `commands.rs` (near `:741`).
- Thread a cheap `is_enriching: bool` into classification: `media_index_file_status` (`commands.rs` ~`:809-817`, next to
  the existing analysis-stamp fetch) reads `scheduler.is_enriching(&volume_id)` (`scheduler/mod.rs:176`) and passes it
  through `classify_file_statuses` → `classify_all` → `classify_one`.
- In `classify_one` (`commands.rs:926-966`), split the `stored == None && covered` branch (`:956-964`): return
  `Indexing` when a pass is running for the volume, else `Pending`. (`excluded` unchanged.)
- Rationale: a move/rename kicks a pass, so the moved file's covered-but-unrowed new path reads "Indexing" (in progress)
  rather than "pending." Reserves `pending` for genuinely-queued files with no active pass. No tombstone, no schema
  change, no new per-path tracking (the scheduler has no per-path queue; only the per-volume running bool).
- Tests: extend the pure-classifier unit tests (`commands.rs` `mod tests`, ~`:1029`) — new-file-with-pass → `indexing`,
  new-file-no-pass → `pending`, and the existing states unchanged.
- FE: add the `indexing` case to `getImageIndexBadge` (`file-list-utils.ts:59-75`, exhaustive switch forces it) with a
  distinct-but-quiet glyph (icon-map already has `rotate-cw`, `hourglass`, `circle-dashed`; pick one that reads
  "working," distinct from `pending`'s `circle-dashed` and `stale`'s `rotate-cw` — propose `loader`/`refresh-cw` style;
  David reviews the glyph). New i18n key `fileExplorer.imageIndex.file.indexing` (draft copy: "Indexing now"). Update
  `file-list-utils.test.ts`. Regenerate specta bindings after the Rust enum change.

## 2. Surface the existing progress indicator inside Settings (per-volume, per-minute)

The top-right hourglass (`IndexingStatusIndicator.svelte`) ALREADY shows image indexing: per-drive rows via
`IndexingEnrichRow.svelte` with "N of M images," image + bytes progress bars, and a **rate (per minute) + ETA** line
(derived FE-side in `eta.ts` from `(done, startedAt)` + a 5 s window). Data feed: `media-enrich-state.svelte.ts`
(`getVolumeEnrichActivity`, per-volume `done/total/bytesDone/bytesTotal/startedAt/paused`).

- LOCKED: keep **per-volume** rows and **per-minute** rate (David prefers both). Do NOT switch to per-second or an
  aggregate.
- Only gap: this summary isn't shown inside Settings. Extract a reusable summary component from `IndexingEnrichRow` (or
  render `IndexingEnrichRow`s directly) and embed it in the "Enable indexing" card (§3), showing the same per-drive
  "N of M images", rate, and ETA whenever a pass is running. No new backend, no new ETA math.

## 3. Restructure `Settings > Indexing > Image indexing` into three cards

Today `ImageIndexingSection.svelte` is ONE `SectionCard`. Split into three (card grouping is section-owned; registry
`section`/`cardKey` stay `['Indexing','Image indexing']`, no new sidebar route):

1. **Enable indexing** — the master `mediaIndex.enabled` `SettingSwitch` + the privacy note + the **live status/progress
   summary** from §2 (per-drive counts + per-minute rate + ETA, shown while indexing) + the `showFileStatusIcons` toggle
   (small display row here).
2. **Folders to index** — everything answering "what gets indexed": `MediaIndexScope` (chosen vs automatic/importance)
   with its `MediaIndexImportanceSlider` + `MediaIndexReclaim`, `MediaIndexChosenFolders` **with per-folder "N of M
   indexed"** badges (wire `media_index_folder_coverage` (`commands.rs:987-1025`) + reuse `getFolderCoverageBadge`; the
   "no cheap per-folder count" comment in `MediaIndexChosenFolders.svelte:17-19` is now obsolete — remove it), the
   **`MediaIndexNetworkVolumes`** SMB per-volume opt-in (MOVED here from its current spot under CLIP — it's about which
   sources get indexed, unrelated to semantic search), and excluded folders.
3. **Semantic search** — `MediaIndexClipModel` only, with a real on/off (see §4) and clarified copy; NO network text.

Update `ImageIndexingSection.a11y.test.ts` and the section's svelte test. New i18n keys for the three card titles +
any summary strings.

## 4. Semantic search: a real on/off toggle + delete-model action (LOCKED: option b)

Today CLIP semantic search has NO on/off and NO uninstall — downloading the model is the de-facto opt-in
(`media_index_clip_model_status` / `media_index_download_clip_model`, `ClipModelStatus` `supported|configured|installed`,
`commands.rs:660-701`). David wants a genuine toggle.

Backend (new work — keep it minimal and correct):
- Add a `mediaIndex.semanticSearch.enabled` setting (registry `definitions/indexing.ts`, `types.ts`, live-applied via
  `settings-applier.ts` → a new `media_index_set_semantic_search_enabled` command + gate atomic, mirroring the existing
  scope/threshold gate atomics in `gate.rs`).
- Gate BOTH the read (`search_semantic` returns `[]` when off) AND the CLIP embedding writes (skip `want_clip` in the
  analyze/enrich path when off) so turning it off stops new CLIP work. Confirm the exact seams:
  `analyze_media(want_vision, want_clip)` and `search_semantic` (`read/` + `scheduler/`).
- Add a `media_index_delete_clip_model` command that removes the downloaded model artifacts + the
  `media_clip_embedding` rows (reclaim disk) and returns to `configured`/`supported` status. Reuse the existing model
  path resolution and the writer's clip-embedding delete primitives. TDD the gate + delete where practical.
- Regenerate specta bindings; wrap the new commands in `tauri-commands/media-index.ts`.

Frontend (in the Semantic search card):
- A toggle bound to `mediaIndex.semanticSearch.enabled`. When on and no model installed, show the Download button
  (existing `MediaIndexClipModel` flow); when installed, show "Enabled" + a "Delete model (reclaim N)" button →
  `media_index_delete_clip_model`. Honest copy: what it does ("search photos by description"), on-device, the model
  download size, and disk reclaimed on delete. Remove the network-drive text from this area.
- Handle the not-supported case (non-Apple-Silicon) as today (disabled with explanation).

## Sequencing (sequential subagents; lead verifies each)

- **WP-A (backend):** §1 honesty state + §4 backend (semantic-search gate + delete-model command). Tests, bindings, TS
  wrappers, `media_index` C+D.md updates. Confirm the parallel-vs-shared-thread enrichment detail for the ETA note.
- **WP-B (frontend):** §1 badge, §2 progress summary in settings, §3 three-card restructure (+ per-folder coverage,
  network-volumes move), §4 semantic-search card UI. All `en` i18n with `@`-descriptions. `pnpm check` (svelte/eslint/
  typecheck).
- **WP-C:** translator fan-out for all new `en` keys (10 locales) + full `pnpm check` + FF-merge readiness.

Copy throughout is DRAFT pending David's review (he reviews all human-facing text).
