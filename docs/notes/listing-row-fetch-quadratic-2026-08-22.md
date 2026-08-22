# The MCP pane mirror re-scans a big listing 100 times per index event

**What this settles:** why a pane parked at the BOTTOM of a large directory burns the main thread and stops answering
IPC, what the real call path is (it is not "the frontend calls `getFileAt` in a loop" for the reason it looks like), and
that it was **never new**: the branch it was found on and `main` reproduced it with the same call chain and the same
ratio.

✅ **It is fixed** (2026-08-22). What shipped, and the before/after on a running app, are at the bottom; the
investigation is kept because the measurement traps in it are the ones anybody re-opening this question will hit first.

**Who it could have hurt, which releases carried it, and whether it recovers on its own**:
`listing-wedge-impact-2026-08-22.md`. It also carries the release-build measurement, which changes the mental model this
note leaves you with twice over: the row-10 cost is neither the scan nor the fan-out but the post-navigation tag
enrichment, and a release build is roughly two orders of magnitude faster than everything measured here.

⚠️ **Both sides are DEBUG builds.** Quote the RATIOS, never the absolute milliseconds or percentages, off this note. ❗
**Release numbers exist now** and they are not close: `listing-wedge-impact-2026-08-22.md` § 2 measured the same probe
on release builds of both sides at six directory sizes, and a release build is **about 96× faster at the bottom of a
20,000-row listing and about 280× faster at row 10**. So the wedge this note describes is a debug-build phenomenon; the
release build at the same size answers a keystroke in 52 ms. Take the mechanism from here and the severity from there.
⚠️ Read `idle-cpu-attribution-2026-08-03.md` first. Its rules were applied here: userspace and file-IO are split
explicitly below, no ordering rests on one `sample` window, and the mechanism was confirmed by reading the code before
any percentage was trusted.

## The mechanism

1. The index emits `index-dir-updated`, or the `/` sentinel that a full-scan completion and a replay overflow use to
   mean "every pane re-enriches" (`file-explorer/pane/index-events.ts`).
2. `createIndexEventHandler` → `hasDescendantUpdate` → `throttledRefresh`, on a **2,000 ms per-pane cooldown** →
   `FilePane.refreshIndexSizes()`.
3. → `debouncedSyncMcp.call()` (**300 ms** debounce) → `syncPaneStateToMcp()` → `buildMcpFileList()`
   (`file-explorer/pane/pane-mcp-sync.svelte.ts`).
4. `buildMcpFileList` loops **up to 100** `getFileAt(listingId, backendIndex, includeHidden)` calls over the visible
   range, one IPC round trip each.
5. `get_file_at` is a **synchronous** `#[tauri::command]` (`commands/file_system/listing.rs`), so Tauri 2 runs it on the
   **main thread**. On macOS it arrives through wry's `url_scheme_handler::start_task`, which is Tauri's IPC transport,
   ❗ **not** `file_viewer/media_protocol.rs`. A sample naming a "URL-scheme handler" here is naming the IPC path. ⚠️
   That frame is the DEBUG transport; a release build carries the same command through the WebKit script-message handler
   (`wry::…did_receive` → `tauri_runtime_wry::create_ipc_handler`), so a probe matching only the first one reads a busy
   main thread as free.
6. `operations::get_file_at` runs `visible_entries(&listing.entries, include_hidden).nth(index)`: a linear scan from 0
   to `index` through a **boxed `dyn Iterator`** (so one vtable dispatch per entry), calling `is_hidden_from_listings`
   on every entry on the way. When the result is `None` it then runs a **second** full `.count()` scan.

So one sync at the bottom of a 74,144-entry listing is 100 × ~74,000 ≈ **7.4 million predicate evaluations on the main
thread**. At the top of the same listing `nth` short-circuits almost immediately and the same sync is free. **Cursor
depth is the multiplier; the index-event rate is the driver.** Neither alone does it.

## What it costs (verified on `worktree-idle-cost` @ `e473e9f62` against `main` @ `6f8499867`, dev builds, macOS 26.5.2 / Darwin 25.6.0, 2026-08-22)

Directory: `apps/desktop/.../target/debug/deps`, 74,144 entries. Cursor position VERIFIED from the DOM at each window.

⚠️ The two CPU percentages just below were read off `ps -M <pid>` row 1, which is NOT the main thread — see §
"Measurement notes". Take them as process-level readings that happen to move with the effect; the `sample` figures
underneath them are the ones that attribute it.

Both windows below ran with the writer queue drained but the importance rescore still ticking about once a second, which
is the app's ordinary resting state on an indexed volume, ❗ not true silence. That tick IS the event source; a
genuinely idle index makes both numbers collapse (see the burstiness note below).

- Cursor at TOP: **2.63 s user / 20 s = 13% of a core.**
- Cursor at BOTTOM: **10.07 s user / 20 s = 50% of a core.**
- While at the bottom, `webview_execute_js` and keyboard IPC **time out at 7 s**. That is the "wedge": the webview is
  alive, the main thread is just never free long enough to answer.

**Userspace against file-IO**, the split `idle-cpu-attribution-2026-08-03.md` demands: main thread cumulative **146.03 s
user against 10.55 s system** (93% userspace) over a 39-minute process; in-window **12.73 s user against 0.20 s system**
(98.4%). The leaf frame is `cmdr_fs::staging::is_staging_temp_name`, a pure string test. ❗ There is no syscall in this
hot path, so none of the usual "it is really IO wait" correction applies.

**The 60 s sample at the bottom**, main thread, 27,379 samples:

```
url_scheme_handler::start_task                    22,240  (81% of the main thread)
 └ AppManager::run_invoke_handler                 22,164
    └ operations::get_file_at                     22,714   (summed over its call sites)
       └ Iterator::nth on Filter<…visible_entries> 15,058
          └ filter_try_fold                       14,399
             └ visible_entries::{closure}         14,381
                └ is_hidden_from_listings         14,227
                   └ is_staging_temp_name         14,204   ← leaf, pure CPU
```

**The control**, a 180 s sample with the pane at the top of a small directory: the main thread is parked in
`ReceiveNextEventCommon` for **78,293 of 78,307 samples**, and `url_scheme_handler` holds 1,963 (2.5%).

⚠️ **It is bursty.** A later 20 s window at the bottom read 5%, because the index had gone quiet and no event fired. ❌
Don't order work off any single window here; the driver is an event rate, not a steady state.

## It is NOT a regression from the idle-cost effort

Two independent checks, and they agree.

**The code is byte-identical to `main`.** `git diff main..HEAD` returns EMPTY for every directory in the call path:
`apps/desktop/src-tauri/src/file_system/listing/`, `apps/desktop/src/lib/file-explorer/pane/`, and
`apps/desktop/src/lib/tauri-commands/`.

**`main` reproduces it**, built and driven through the identical protocol in a second worktree:

|                                       | branch           | `main`          |
| ------------------------------------- | ---------------- | --------------- |
| main-thread samples (60 s, at bottom) | 27,379           | 32,387          |
| `url_scheme_handler::start_task`      | 22,240 (81%)     | 31,722 (98%)    |
| `operations::get_file_at`             | 22,714           | 28,389          |
| `is_staging_temp_name` (leaf)         | 14,204           | 17,418          |
| top → bottom, one window each         | 13% → 50% (3.8×) | 6% → 23% (3.8×) |
| IPC times out at the bottom           | yes              | yes             |

The **ratio is the comparable figure** (3.8× on both); the absolute percentages differ only because the `main` instance
was still doing its first index scan, which moves the process baseline without touching this path.

**The effort's sync-status change is ruled out as an interaction**, which was the one way identical code could still
have behaved differently: no sync-status frame appears anywhere in the main-thread hot path on either build, and the
probe runs on its own `cmdr-sync-status` threads.

## Recommended fixes, as they were ranked before any were applied

1. **Give the listing a cached visible-index map.** A `Vec<u32>` of visible positions per `(listing, include_hidden)`,
   invalidated when the entries or the two staging-visibility settings change, turns `nth(index)` into an array lookup.
   It fixes the root cost for every accessor that shares `visible_entries` (`find_file_index`, `find_file_indices`,
   `get_file_beside`, `get_paths_at_indices`, `get_total_count`), not just this caller. Best value, and contained to
   `file_system/listing/`.
2. **Ask for the range in one call.** `buildMcpFileList` fetches a CONTIGUOUS range one row at a time;
   `get_file_range(listingId, start, count, includeHidden)` already exists and `pane/entries-snapshot.ts` already uses
   it. One call replaces up to 100, collapsing 100 scans into one walk. Independent of (1), and cheap.
3. **Take the listing read commands off the main thread.** `get_file_at`, `get_file_beside`, `get_total_count`, and
   `get_listing_stats` are all synchronous `#[tauri::command]`s, so Tauri runs them on the main thread and any slow one
   stops the app answering IPC at all. Making them `async` doesn't remove the waste, but it turns a wedge into slow
   rows, which is what principle 2 ("never block the main thread") actually asks for.
4. **Drop the second `.count()` scan** in `get_file_at`'s `None` branch, or serve it from a cached visible count. It
   doubles the cost exactly in the FE/BE drift window where these calls are already missing.
5. **Question whether the mirror should run with no MCP client attached.** `syncsToMcp` is a VOLUME CAPABILITY
   (`pane/volume-capabilities.ts`), not a check that anything is subscribed, so this work happens for every user on
   every local volume whether or not an agent is connected. Gating it on a live subscriber removes the cost rather than
   shrinking it. Product call, hence last.

(1) and (2) together remove essentially all of it; (3) is the safety net that stops any future O(n) read from wedging
the window.

## Measurement notes for whoever re-runs this

- **Isolate the main thread.** Process-wide CPU is useless here: a window measured 0% on the main thread while the
  process sat at 126%, all of it index-writer churn on other threads. ❌ But **`ps -M <pid>` cannot do the isolating**:
  it prints no thread names and its rows are not ordered, so the first one is not the main thread. Reading it that way
  reported 0% while a `sample` of the same process, in the same seconds, found the main thread 98% inside the IPC
  handler. `sample` names the main thread (`DispatchQueue_1: com.apple.main-thread`) and is the only one of the two that
  can answer this. `scripts/main-thread-ipc-share.py` does the counting.
- **Verify the cursor position from the DOM at the measurement window**, not from the keystroke you sent. A listing
  refresh resets the scroll to the top, which silently turned one "at the bottom" reading in this investigation into a
  top-of-listing reading and briefly refuted a correct hypothesis.
- **Creating a worktree while the app runs is a measurement hazard.** An APFS `target/` clone lands thousands of files
  on an indexed volume; it drove the writer queue to 12,291 and added ~80 s of writer CPU here.
- **Measuring a RELEASE build brings three more traps**, all of which produced a wrong answer first:
  `main-thread-ipc-share.py` matched only the debug IPC transport (fixed in `0ff4890a6`), App Nap on a lid-shut machine
  inflated a latency by 20×, and hammering `move_cursor` back to back lets the 300 ms debounce coalesce the syncs the
  probe is trying to time. Full protocol and the numbers behind each: `listing-wedge-impact-2026-08-22.md` § "The § 2
  release benchmark, and how to re-run it".

## What shipped (2026-08-22)

(1), (2), (3), and (4) all did. (5) is still open and still a product call.

- **A materialized row map per listing** (`listing/visible_rows.rs`), so every accessor indexes instead of walking, and
  `entries` is private with `entries_mut()` dropping the map on the way out. Design and the reason the map splits
  settled rows from scratch-named candidates: `apps/desktop/src-tauri/src/file_system/listing/DETAILS.md` § "Row
  numbers".
- **One `getFileRange` for the mirror's visible range** instead of up to 100 `getFileAt` calls
  (`pane/pane-mcp-sync.svelte.ts`).
- **Every listing read command is `async`**, so none of them runs on the main thread; `refresh_listing_index_sizes` goes
  onto the blocking pool because it runs indexed SQLite queries.
- **The second `.count()` scan is gone** from the out-of-bounds path.

It also fixed three accessors that had grown their own filter and disagreed with `getFileAt` about what a row number
means — type-to-jump could put the cursor a row off while a copy ran in the directory. The compile errors from making
`entries` private are what found them.

### Before and after, on the running app (dev/debug builds, macOS 26.5.2 / Darwin 25.6.0, 2026-08-22)

Directory: `.../target/debug/deps`, 19,251 rows before and 19,513 after (the rebuild between the two runs added files).
⚠️ Smaller than the 74,144-entry directory the investigation used, so these numbers are NOT comparable to the ones above
— read them only against each other. **The index was OFF for every volume in both runs**, which makes the result
stronger than expected: no index storm is needed to wedge the app, because a cursor move drives the mirror by itself.

`move_cursor` over Cmdr's own MCP is the probe: it is what the user's arrow key does, and the tool answers anyway after
5 s if the frontend never acknowledges, so a timing at that ceiling means the app never answered. 15 moves per depth, in
alternating blocks (`scripts/cursor-move-latency.py`).

|                                     | before             | after                      |
| ----------------------------------- | ------------------ | -------------------------- |
| cursor moves answered at row 10     | 1 of 15 (3,394 ms) | **15 of 15, median 6 ms**  |
| cursor moves answered at row 19,100 | **0 of 15**        | **15 of 15, median 23 ms** |

Main thread under a matched deep cursor-move load, 10 s `sample` (`scripts/main-thread-ipc-share.py`): **98.7% → 7.2%**
of main-thread samples inside `url_scheme_handler::start_task`. Samples with a leaf inside the visibility scan
(`scripts/listing-scan-leaves.py`): **1,203 → 150** — and the "after" window carried roughly 200× more cursor moves,
because each takes ~20 ms rather than never finishing.

Under a deliberately unfair sustained hammering (1,500 back-to-back moves, no pause, overlapping the `sample`'s own
suspensions) the fixed build still answered 295 of 300 within 5 s at a median of ~500 ms. Nothing a person does
approaches that rate.

**Correctness in the app, not just in tests**: with the cursor on row 19,513 of 19,513, `cmdr://state` shows
`zune_jpeg-…-cgu.3.rcgu.o` under it, which is the last name `ls | sort` gives for that directory.
