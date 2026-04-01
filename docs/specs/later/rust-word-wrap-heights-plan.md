# Move word-wrap height calculation from JS (pretext) to Rust

## Context

We built accurate word-wrap virtual scrolling using the `@chenglou/pretext` JS library, but it only works for FullLoad files (<1MB) because it needs all line text on the frontend. Moving the calculation to Rust — where the text lives — enables it for LineIndex files too (multi-MB), which is the real win.

The viewer uses a monospace font, so all characters have the same width. We only need **character counts per line** from Rust. The frontend builds the prefix-sum locally using `char_count * char_width_px / available_width_px`. This means resize-triggered reflow is instant (no IPC), which is better than the current pretext approach.

## What changes

### Rust: collect char counts during file scan

**`full_load.rs`**: Add `char_counts: Vec<u32>`. Populate during `open()` while splitting lines (already iterating — trivial cost).

**`line_index.rs`**: Add `char_counts: Vec<u32>`. Count chars during the existing memchr scan — each line segment between newlines is decoded and counted. Adds ~30% scan time but avoids a second pass. The 5s indexing timeout still applies; if char counting pushes it over, the session stays in ByteSeek (no regression).

**`byte_seek.rs`**: No change. Returns `None` for char counts.

**Add trait method** to `FileViewerBackend`:
```rust
fn char_counts(&self) -> Option<&[u32]>;
```

### Rust: new IPC command

**`viewer_get_char_counts(session_id) → Option<Vec<u32>>`**

Returns per-line character counts. `None` for ByteSeek. For FullLoad: immediate. For LineIndex: available after indexing completes.

No prefix-sum computation in Rust — the frontend does that with the char_width and available_width it knows locally.

### Frontend: rewrite `viewer-line-heights.svelte.ts`

Same external API (`ready`, `getLineTop`, `getLineAtPosition`, `getTotalHeight`, `reflow`, `cancel`), completely different internals:

- `loadFromRust(sessionId)` — calls `viewerGetCharCounts`, stores `Uint32Array`
- `buildPrefixSum(charWidthPx, availableWidthPx)` — local arithmetic: `visual_lines = max(1, ceil(cc * charWidth / availWidth))`, builds `Float64Array` prefix-sum
- `reflow(charWidthPx, newWidth)` — rebuilds prefix-sum from cached char counts. **No IPC** — instant.
- Replace `resolveAndValidateFont()` (93 lines, canvas + DOM validation) with `measureMonoCharWidth()` (~10 lines, single DOM measurement)
- Remove pretext dynamic import, `PreparedText`, `requestIdleCallback` scheduling, font validation constants

### Frontend: update `viewer-scroll.svelte.ts`

- **Remove dep**: `getAllLines` — no longer needed
- **Add dep**: `getCharWidthPx` — measured once from DOM
- `runHeightMapInitEffect`: trigger when `wordWrap` on + `backendType` is fullLoad OR lineIndex (not just fullLoad!) + `charWidthPx > 0` + `textWidth > 0`. Call `heightMap.loadFromRust(sessionId)` then `heightMap.buildPrefixSum(charWidthPx, textWidth)`.
- `runHeightMapReflowEffect`: call `heightMap.reflow(charWidthPx, newTextWidth)` — purely local, no IPC

### Frontend: update `+page.svelte`

- **Remove** the FullLoad "fetch all lines" hack (was only needed to feed pretext)
- **Remove** `viewerGetLines` import (was only added for the hack)
- **Add** `charWidthPx` state: measured once on mount via a hidden span with `var(--font-mono)` at `var(--font-size-sm)`
- Wire `getCharWidthPx` dep to scroll composable

### Remove pretext

- Remove `@chenglou/pretext` from `package.json`
- Remove `pretextModule` / `pretextReady` / dynamic import from `viewer-line-heights.svelte.ts`
- Remove from `pnpm-lock.yaml` via `pnpm install`

## What stays the same

- Scroll integration: `heightMap.ready` gates all paths, falls back to averaged heights
- `scrollScale` / `MAX_SCROLL_HEIGHT` handling
- Search composable (`viewer-search.svelte.ts`) — completely unchanged
- `runWrappedLineHeightEffect` — still needed as fallback for ByteSeek
- CSS rules for word wrap

## Known edge cases

- **Tabs**: `chars().count()` counts tab as 1 char but CSS renders it as `tab-size` chars (typically 4 or 8). Count each tab as `tab_size` during the Rust scan — it's one branch in the counting loop, trivial cost. Without this, tab-heavy files (Go, Makefiles, C) would have visible scroll drift on every indented line.
- **CJK double-width**: Undercounts by up to 50% for CJK-heavy lines. Can use `unicode-width` crate later.
- **Emoji ZWJ**: Overcounts multi-codepoint emoji. Acceptable — same magnitude as old averaged approach.
- **IPC size for large files**: 1M lines = 4MB of `u32` values. Serialize as a comma-separated string (not JSON array) — avoids JSON encoding overhead on both sides. 7MB is the upper bound we're willing to transfer; above that, fall back to averaged heights.

## Implementation order

1. Rust: `char_counts` in FullLoadBackend + trait method + IPC command
2. Frontend: rewrite `viewer-line-heights.svelte.ts` (char counts from Rust + local prefix-sum)
3. Frontend: update `viewer-scroll.svelte.ts` and `+page.svelte` (remove getAllLines, add charWidthPx, remove fetch hack)
4. Rust: `char_counts` in LineIndexBackend (the real win)
5. Frontend: activate height map for lineIndex backend too
6. Remove pretext dependency
7. Tests + CLAUDE.md updates

At no point in this sequence is functionality lost.

## Verification

- Open the word-wrap drift test file (`_ignored/test-data/word-wrap-drift-test.txt`), press W, press Home/End — no drift
- Open a >1MB file with variable-length lines, toggle word wrap, scroll — accurate positions
- Resize the viewer window while word-wrapped — instant reflow, no jitter
- Open a file that stays in ByteSeek mode — averaged heights still work as before
- `./scripts/check.sh` passes

## Critical files

- `apps/desktop/src-tauri/src/file_viewer/full_load.rs` — add `char_counts`
- `apps/desktop/src-tauri/src/file_viewer/line_index.rs` — add `char_counts` during scan
- `apps/desktop/src-tauri/src/file_viewer/mod.rs` — trait method, types
- `apps/desktop/src-tauri/src/commands/file_viewer.rs` — new IPC command
- `apps/desktop/src/routes/viewer/viewer-line-heights.svelte.ts` — complete rewrite
- `apps/desktop/src/routes/viewer/viewer-scroll.svelte.ts` — update deps/effects
- `apps/desktop/src/routes/viewer/+page.svelte` — remove hack, add charWidthPx
- `apps/desktop/package.json` — remove pretext
