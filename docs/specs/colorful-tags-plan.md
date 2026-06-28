# Colorful tags (macOS Finder tags) — implementation plan

**Status:** planning · **Branch:** `colorful-tags` · **Created:** 2026-06-28

Read [`docs/design-principles.md`](../design-principles.md) and [`apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md`](../../apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md) (+ its `DETAILS.md`) before executing.

## Goal

Read and write macOS Finder tags (`com.apple.metadata:_kMDItemUserTags`) and show Finder-style colored dots in the file panes, fully Finder-compatible (tags Cmdr writes appear correctly in Finder and vice versa). Two phases:

- **Phase 1 — read + display.** Colored dots next to file names. No writing.
- **Phase 2 — assign.** A right-click context-menu action toggling the seven system colors, with assignable keyboard shortcuts (none bound by default).

Deliberately **out of scope** (later): a "Tags…" picker popup, creating custom-named tags, a tag sidebar, filter/sort by tag, and a separate Tags column. Keep it minimal.

## Why this shape (the load-bearing research)

The decisions below rest on findings already gathered. Don't re-litigate without new evidence.

- **Tag format.** `_kMDItemUserTags` is a **binary** plist holding an array of strings, each `"Name\nN"` where `N` is the color index. Color map (verified, Eclectic Light Company, 2017): `0`=none (no dot), `1`=grey, `2`=green, `3`=purple, `4`=blue, `5`=yellow, `6`=red, `7`=orange. A named tag stores its color the same way; a colorless tag is just `"Name"` (treat as color 0). The per-file xattr is the display source of truth: when a user recolors/renames a tag, Finder rewrites every file's xattr, so we never need the system tag registry for Phase 1/2.
- **We don't load any of this today.** The live local-disk listing path is `Volume::list_directory` → `list_directory_core` (`local_posix.rs`): `read_dir` + one `lstat` per entry, no NSURL, no xattr. The richer `list_directory()` and the deferred `get_extended_metadata_batch()` in `reading.rs` sit under `#![allow(dead_code)]` ("two-phase loading API") — designed but dormant; `added_at`/`opened_at` are unpopulated in the live flow.
- **Two different cost classes — keep them separate.** Measured on this laptop, warm, synthetic 200k-file dir, 2026-06-28: baseline `read_dir`+`lstat` ≈ 500 ms; adding a **`getxattr` per file for tags** adds ~2.4–3.3 s → **~15 µs/file** (≈6× the per-entry `lstat`; `_kMDItemUserTags` goes through the `com.apple.metadata` namespace). Separately, the dormant lane's **`added_at`/`opened_at` via NSURL `getResourceValue` is Tier 3 ~50–100 µs/file** (`listing/DETAILS.md:170`; XPC round-trip, and `src-tauri/CLAUDE.md` forbids NSURL on rayon — XPC blows the 2 MB worker stack). So tags are the cheap headline; the NSURL dates are 3–7× heavier and **not** cheaply parallelizable. **Don't conflate them into "one free pass."** See DP1.
- **Both fetch visible-range-first, never eager/inline over the whole listing.** Visible range (~100 rows) ≈ 1.5 ms for tags; a full 200k tag sweep ≈ 3 s. The existing in-codebase precedent for a visible-range `getxattr` is the **custom-folder-icon prefetch** (`file-list-utils.ts` prefetch on `visibleDirPaths` → the `kHasCustomIcon` `getxattr`, `views/DETAILS.md:132`), *not* the git column (which fetches the whole dir in one fast subprocess). Mirror the icon prefetch.
- **Context menu can show colored circles with no fork.** The file context menu is a native NSMenu via muda (`commands/ui.rs` `show_file_context_menu` → `menu_structure.rs` `build_context_menu` → `menu.popup`), and it **already** renders arbitrary RGBA images — the "Open with" submenu does it (`menu/open_with.rs`: `Image::new_owned(rgba,w,h)` → `IconMenuItem::with_id`). Stock muda's `IconMenuItem` has no native gutter checkmark (a fork would be a two-repo muda+Tauri patch); we **composite the checkmark into the bitmap** for applied tags — exactly David's mockup (checkmark inside the circle). No fork.
- **Deps already present.** `plist = "1.8.0"` (binary plist; **note it defaults to XML — encode with `to_writer_binary`**) and `xattr = "1"` (raw `getxattr`/`setxattr`, used by `icons/per_path.rs`). No new crate.

## Architecture decisions

**D1 — Decouple tags (cheap getxattr) from `added_at`/`opened_at` (Tier 3 NSURL).** They share the "deferred, visible-range-first" shape but **not** a cost budget or a thread model. Tags get a dedicated cheap path; reviving the NSURL dates is a separate, optional sub-track (see DP1). *Why:* the 15 µs benchmark only justifies the tag pass; bundling 50–100 µs NSURL work behind the same "free" framing would make a 200k backfill ~20 s and can't ride the cheap parallelization. Tags are the headline; don't let dates inflate the hot path.

**D2 — Tags live on `FileEntry` + the listing cache (visible range), not a side-map.** Add `tags: Vec<TagRef>` to `FileEntry` (`listing/metadata.rs`). A dedicated `set_entry_tags(listing_id, path, tags)` helper in `caching.rs` looks the entry up by path under the write lock, mutates `.tags` in place (tags are never sort-relevant, so skip the `update_entry_sorted` sort-relevance branch and the full-`FileEntry` reconstruction), then reuses the existing mutator tail: `touch()` (already automatic inside the mutators, `caching.rs:381`) + one coalesced `enqueue_diff("modify")`. The modify-diff doubles as the render + width-settle trigger. *Why:* single source of truth (the cache), reuses the diff machinery, makes future sort/filter-by-tag natural, and gives the carry-forward fix (D3) a natural home. *(Note: `notify_modified` is the caller of `update_entry_sorted`+`enqueue_diff`, not the reverse — the bespoke helper sidesteps that whole path.)*

**D3 — Carry cached tags forward across watcher re-stats (else they get erased).** The listing watcher's incremental path re-stats a changed entry via `get_single_entry` (no xattr → empty `tags`) and **wholesale-replaces** the cached entry (`update_entry_sorted` does `entries[idx] = new_entry`, `caching.rs:402`; same in `notify_directory_changed::Modified`, `caching.rs:458`). So without a fix, **any** Modify event on a visible file (content edit, mtime touch, chmod — not just tag changes) blanks its tags until re-enrich. Fix (single edit): both modify paths funnel through `update_entry_sorted` (the incremental watcher applies its `modifies` there, `watcher.rs:314`; `notify_directory_changed::Modified` → `notify_modified` → `update_entry_sorted`, `caching.rs:557`), which already binds `old` right before overwriting (`caching.rs:384`). So one line there — `if new_entry.tags.is_empty() { new_entry.tags = old.tags.clone() }` — covers both sites and can't be half-applied. (The `adds` path uses `insert_entry_sorted`, `watcher.rs:304` — a genuinely new entry with no old tags, correctly left empty for enrich.) Pairs with the "removals propagate" rule in M0's `enrich_tags`: carry-forward keeps tags sticky across unrelated re-stats; an applied empty read is what clears them on a real removal. *Why:* makes tags sticky regardless of enrich timing — principle 3 (rock solid), and it's the robust form of "refresh on external change." This is M0 work, not a later milestone.

**D4 — Backfill (off-screen rows) must NOT spray per-entry diffs.** If we ever backfill the whole listing (DP2), don't patch off-screen cache entries with `"modify"` diffs — each diff makes `FilePane` re-fetch the visible range and recompute column widths for no visible change (~the 50 ms-coalesced "modify" cost, repeated for a 3 s sweep). Backfill into a quiet store (or batch without emitting) and only surface rows as they scroll in. *Why:* principle 5 (respect resources). Both existing analogs (git status, custom icons) keep off-screen work in a side path for this reason.

**D5 — Both fetches gated to local volumes, and the backfill tied to listing lifecycle.** `getxattr`/NSURL on an SMB/MTP mount can block indefinitely (`src-tauri/CLAUDE.md` network-mount rule); tags are macOS-local-only anyway. Enrichment runs only on local-disk backends (empty `tags` elsewhere), wrapped in `blocking_with_timeout`, and any background backfill checks the `listing_id` still exists in `LISTING_CACHE` / isn't past `list_directory_end` each batch so navigating away stops it (the per-listing AtomicBool/`Notify` cancel machinery doesn't reach a separate task). *Why:* principle 3 (handle the hostile mount), 5 (don't burn CPU on an abandoned sweep).

**D6 — Per-file xattr is the display source of truth; no system tag registry.** Parse each file's own `_kMDItemUserTags` for `(name, color)`. Phase 2 writes only the seven canonical system color tags (`Red\n6`, …), which need no registry.

**D7 — Composite the applied-state checkmark into the circle bitmap.** No muda/Tauri fork. Generate circles at 2× (~36 px square; muda fixes menu images to 18 pt logical) with a 1 px border baked in; non-template images don't auto-tint, so the border keeps a pale dot legible on light/dark menus and the selection highlight. RGBA generation is pure CPU — no `MainThreadMarker` needed (same path open_with already uses).

**D8 — On-brand colors, light/dark.** Seven `--color-tag-*` tokens in `src/app.css` (light + dark), each dot drawn with a 1 px border. Index `1–7` → token; index `0` → no dot. Only colored tags produce dots.

**D9 — Display placement & overflow.** Dots cluster right-aligned within the **Name** field, overlapping (ForkLift-style). Overflow rule: up to 3 dots; when the colored-tag count is `>3`, show 2 dots then a faint `+N` chip in the third slot where `N = count − 2`. Applies to Brief and Full mode. When "show file extensions in the name column" is **off** (separate Ext column), tags still render in the Name column for now (a later refinement moves them after the extension).

**D10 — Column width includes the tag cluster, with an async settle.** Width becomes `max_row(nameWidth + clusterWidth + padding)` (`measure-column-widths.ts` `computeFullListColumnWidths`; Brief `compute_brief_column_text_widths`). `clusterWidth` is a pure function of the capped dot count — folds into the canvas measurement, no DOM reflow. Tags arrive after first paint, so render name-only width instantly, then recompute (grow-only, debounced) once the tag batch lands — one quiet settle per directory (David accepts it).

**D11 — Write safety: never wholesale-null `com.apple.FinderInfo`.** That 32-byte blob carries `kHasCustomIcon` (bit `0x0400` at offset 8, `icons/per_path.rs`) plus type/creator codes; zeroing it destroys custom folder icons and breaks Cmdr's own `has_custom_folder_icon`. **Default: don't touch FinderInfo at all** — modern Finder reads `_kMDItemUserTags` directly for the dot. Only if round-trip testing proves Finder needs the legacy label, read-modify-write **just** the 3 label-color bits, preserving everything else. *Why:* principle 4 (protect the user's data) — this is the sharpest data-loss edge in the feature.

## Milestones

Sequential is fine; we're not in a hurry. Each ends with the named checks.

### M0 — Backend: tag parsing + cheap deferred tag pass
- `listing/metadata.rs`: add `TagRef { name: String, color: u8 }` (specta::Type, camelCase) and `tags: Vec<TagRef>` on `FileEntry` (default empty, cross-platform).
- New `file_system/tags.rs` (+ sibling `CLAUDE.md`/`DETAILS.md`): `read_tags(path) -> Vec<TagRef>` — `xattr::get(path, "com.apple.metadata:_kMDItemUserTags")`, decode the **binary** plist with `plist` into `Vec<String>`, split each on the final `\n` into `(name, color 0–7)`, default 0 when absent/unparseable. **`#[cfg(target_os = "macos")]` for the real impl; a non-macOS no-op returning `[]`** so it compiles everywhere.
- New command `enrich_tags(listing_id, paths)` (async, `blocking_with_timeout` per `commands/CLAUDE.md`): **local-volume only** (D5, gate via `supports_local_fs_access()` / `local_path().is_some()`); reads tags for the batch, patches cache via `set_entry_tags`, no-op on non-local backends. **Removals propagate:** enrich applies **empty** reads too, and `set_entry_tags` replaces unconditionally (including to `[]`) — otherwise the D3 carry-forward holds phantom dots forever after a user clears all tags in Finder (carry-forward keeps the old tags; only an applied empty read clears them).
- `caching.rs`: the `set_entry_tags` helper (D2: look up by path under the write lock, mutate `.tags` in place, `touch()`, one coalesced diff).
- **Carry-forward (D3):** in the incremental watcher path (`watcher.rs`) and `notify_directory_changed::Modified` (`caching.rs:458`), copy the existing cached entry's `tags` onto the re-stat'd replacement entry so an unrelated Modify event doesn't blank them. This is the robust core of "refresh on external change" — it makes tags survive every re-stat.
- **`added_at`/`opened_at` revival is a separate sub-track (DP1)** — not bundled into `enrich_tags`. If pursued, either its own heavier deferred command or a migration to a raw `getattrlist(ATTR_CMNEXT_ADDEDTIME)` syscall to stay cheap.
- **Tests (TDD, real red→green for the parser):** `tags.rs` decode — empty → `[]`; `"Red\n6"`; multiple; colorless `"Work"` → 0; malformed trailing bytes → no panic, 0; a **captured real Finder bplist fixture** decodes to expected `(name,color)`. `set_entry_tags` test: updates entry, enqueues exactly one diff, bumps `last_accessed_ms`. **Carry-forward test (D3):** a Modify event on a tagged visible entry preserves its `tags` after re-stat. **Guard test: `list_directory_core` performs zero `getxattr`** (lock the hot path so a refactor can't drag xattr reads back into the bulk listing).
- **Docs:** `tags.rs` `CLAUDE.md`/`DETAILS.md`; `listing/DETAILS.md` (the tag pass, why visible-range-first, the 15 µs/file anchor with date, the explicit tags-vs-NSURL cost split); guardrail line in `listing/CLAUDE.md` only if a missing diff/touch can silently break.
- **Checks:** `pnpm check rust`, `pnpm check clippy`, `pnpm bindings:regen`.

### M1 — Frontend: colored-dot display
- Regenerate bindings; `FileEntry.tags` flows to the frontend.
- `src/app.css`: seven `--color-tag-*` tokens, light + dark.
- A `TagDots.svelte` cell renderer (D8/D9): overlapping bordered dots, the `+N` rule, colorless skipped, tag names as accessible labels (AA, screen-reader per principles).
- Wire into `FullList.svelte` + `BriefList.svelte` Name cells; **trigger `enrich_tags` for the visible range on scroll, mirroring the custom-folder-icon prefetch** in `file-list-utils.ts` (debounced), not the whole-dir git trigger.
- `measure-column-widths.ts` + Brief width calc: include cluster width (D10); recompute-and-settle when tags arrive.
- Wire `enrich_tags(listingId, visiblePaths)` into `fetchVisibleRange` (`file-list-utils.ts`, next to `prefetchCustomFolderIcons` / `onSyncStatusRequest`) so it also re-fires after any `directory-diff` refetch — that re-fire plus the D3 carry-forward together keep dots correct without a bespoke scroll listener.
- **Tests:** Vitest for the `+N`/colorless/cap logic and cluster-width math; a11y test for dot labels; width-settle snapshot. Playwright E2E (read-only): seed a temp dir with files tagged via `xattr` in the fixture, assert dots render with the right count/colors.
- **Docs:** colocated `CLAUDE.md`/`DETAILS.md` for the view changes (note the async width settle).
- **Checks:** `pnpm check desktop`, targeted `pnpm check desktop-e2e-playwright`.

### M2 — Phase 2: context-menu assign (seven colors, toggle)
- Rust circle-bitmap generator (cache the 7×{normal,checked} RGBA images): on-brand colors + 1 px border, 36 px square, checkmark composited for the checked variant (D7). In `tags.rs` or `menu/tag_icons.rs`.
- `menu_structure.rs` `build_context_menu`: a tags group of seven `IconMenuItem`s (open_with.rs pattern), checked variant when the selection already carries that color; click toggles.
- **`menu_id_to_command` is an exhaustive manual match (menu `CLAUDE.md`)** — add static IDs for the seven items (or a `tag:` prefix-match path like `open-with:`). Don't only touch `command-registry.ts`/`command-ids.ts`.
- Seven "toggle tag <color>" command IDs, assignable, **no default shortcut**. Selection semantics reuse the existing right-click selection set.
- **Tests:** bitmap generator (dimensions, deterministic bytes, checked ≠ normal); menu-build test (seven items, correct checked state for a given tag set). Manual: David QAs the popup visually (memory: don't lean on screenshot MCP).
- **Docs:** `menu/DETAILS.md` (tag items, why composite checkmark not gutter, the muda+Tauri fork avoided, macOS-only — Linux has no menu icons).
- **Checks:** `pnpm check rust`, `pnpm check svelte`, `pnpm check clippy`.

### M3 — Write path + Finder consistency (the risky one)
- `tags.rs`: `set_tags(path, Vec<TagRef>)` — **read-modify-write preserving other tags** (add/remove only the toggled color), encode with **`plist` binary** (`to_writer_binary`), `xattr::set`. **FinderInfo per D11: default untouched; if proven necessary, label-bits-only RMW, never a null blob.** Spotlight `mdimport` nudge only if round-trip testing shows tag search needs it.
- TOCTOU: the RMW window is acceptable for tags (low stakes); `setxattr` is atomic per attribute so no partial-write risk. Note it.
- **Tests (TDD for encode/decode; hard-test the data-writing path, principle 4):** bplist encode→decode **semantic** round-trip written test-first (decode equality, **not** byte-for-byte vs a Finder reference — valid bplists differ in object-table ordering/dedup). Integration: Cmdr writes tags → re-read decodes to the same set; a custom-icon folder keeps `kHasCustomIcon` after tagging (regression for D11). **Manual fidelity (human-verified, principle 6):** (a) tag from Cmdr → Finder shows dot/color/name + appears in Finder tag search; (b) tag from Finder → Cmdr reads identical; (c) add/remove preserves other tags; (d) custom folder icon survives.
- **Docs:** `tags.rs` `DETAILS.md` — the exact Finder-consistency side-effects found necessary, evidence-anchored (macOS version + date).
- **Checks:** full `pnpm check`, then `--include-slow` before wrap.

### M4 — Re-enrich on external tag change
With D3 carry-forward (tags survive re-stat) and M1's `enrich_tags`-on-`fetchVisibleRange` (re-fires after any `directory-diff` refetch), an external tag change on a **visible** row already self-heals: the watcher's Modify event triggers a diff → the frontend refetches the visible range → `enrich_tags` re-reads the new tags. So most of "refresh on external change" falls out of M0+M1 for free.
- The one gap: a tag change that produces **no** notify Modify event for that path leaves the dot stale until the next visible fetch. The listing watcher consumes notify-rs `EventKind` (`watcher.rs`), **not** raw FSEvent flags — so don't target `ItemXattrMod`/`FINDER_INFO_MOD` from `crates/fsevent-stream` here (that's the *indexing* watcher's crate); a `_kMDItemUserTags` write surfaces as `EventKind::Modify(ModifyKind::Metadata | Any)`. **Verify empirically** that a Finder tag change emits such an event for the file; if it does, no extra work — M0+M1 cover it. If it doesn't, add a targeted re-enrich of the visible range on `Modify(Metadata)` events.
- **Decision:** likely a no-op milestone (covered by M0+M1); kept explicit to force the empirical check. *DP3.*
- **Tests:** integration test that a `Modify(Metadata)` event on a visible tagged path results in refreshed tags (via the diff→refetch→enrich loop). **Checks:** `pnpm check rust`.

## Decision points for David

- **DP1 — `added_at`/`opened_at` revival.** Tags are cheap (15 µs) and ship on their own pass. The dormant NSURL dates are Tier 3 (50–100 µs, can't cheap-parallelize). Options: (a) skip dates for now, ship tags only; (b) revive dates as a separate heavier deferred lane; (c) migrate dates to a raw `getattrlist` syscall so they're cheap too. Recommend (a) now / (c) when you actually need the dates, so the tag hot path stays clean. *(You said you'll want the dates "soon" — (c) is the elegant end state, but it's its own small piece of work.)*
- **DP2 — Backfill appetite.** Ship **visible-range-only** first (snappy; sort/filter-by-tag would only see loaded rows), or add the cancelable local-volume background backfill now (~3 s/200k, parallelizable to <1 s across ~4 threads — true for tags since `getxattr` is a raw syscall)? Recommend visible-range-first; backfill when a feature needs it. (Backfill must obey D4 + D5.)
- **DP3 — External-change refresh (M4).** Mostly free via M0 carry-forward + M1's enrich-on-visible-fetch. The only open question is the empirical one: does a Finder tag change emit a notify `Modify(Metadata)` event for the file? If yes, nothing to do; if no, add a targeted visible-range re-enrich. Recommend running the check and wiring the small fallback only if needed; either way, no stale-until-re-navigation regression.
- **DP4 — Show/hide setting.** The git column ships behind `fileExplorer.git.showStatusColumn` (default off). Tags: always-on like Finder, or a visibility toggle? Recommend always-on (it's lightweight and the point is delight), revisit if users want it off.
- **DP5 — Light/dark menu circles.** One bordered circle set (you leaned this), or separate light/dark menu bitmaps? Recommend one bordered set; revisit only if a pale color washes out on a real menu background.

## Notes
- Parallelism: M0 and the `src/app.css` token addition (start of M1) are independently safe; everything else sequential.
- The throwaway bench lives at `/tmp/cmdr-xattr-bench/` (`rustc -O bench.rs && ./bench 200000 0.10`); `rm -rf` anytime.
