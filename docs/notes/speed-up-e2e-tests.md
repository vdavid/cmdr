# E2E test speed-up

Tracking work to make the Playwright E2E suite faster.

Status: in progress. Started 2026-05-12.

## Before

- Date: 2026-05-12
- Machine: macOS, native, single worker
- Branch: `e2e-speedup` (worktree)
- Suite result: 121 passed, 17 skipped (SMB on macOS), 1 failed
- Failed: `mtp.spec.ts:784` (`MTP read-only enforcement › read-only storage rejects write operations`): flaked at 19.9s
  on a `has_item` poll inside `mcpAwaitItem` for `sunset.jpg` (15s mcp timeout); see triage at bottom

### Totals

- **Playwright wall-clock**: 611.5 s ≈ **10m 12s** (`stats.duration` in the JSON report)
- **Checker total** (incl. build + app start + cleanup): **13m 12s**
- **Sum of per-test durations**: 608.8 s (virtually all of Playwright's time is inside tests; ~3s is reporter/teardown
  overhead)
- **Total fixed-sleep budget**: **488.9 s** across 2299 `sleep()` calls (**80% of wall-clock is fixed sleeps**)

That last number is the headline: four out of every five seconds the suite runs, it's sitting in a `setTimeout`.

### Per-file durations

| File                        | # tests | Total (s)              |
| --------------------------- | ------- | ---------------------- |
| mtp.spec.ts                 | 21      | 162.6                  |
| accessibility.spec.ts       | 20      | 150.6                  |
| file-watching.spec.ts       | 11      | 87.4                   |
| mtp-conflicts.spec.ts       | 5       | 50.4                   |
| conflict-edge-cases.spec.ts | 7       | 45.8                   |
| app.spec.ts                 | 14      | 31.6                   |
| network-toggle.spec.ts      | 4       | 17.2                   |
| error-pane.spec.ts          | 4       | 15.4                   |
| conflict-copy.spec.ts       | 7       | 14.6                   |
| file-operations.spec.ts     | 8       | 14.1                   |
| conflict-move.spec.ts       | 3       | 5.8                    |
| indexing.spec.ts            | 3       | 5.6                    |
| git-portal.spec.ts          | 4       | 3.7                    |
| viewer.spec.ts              | 10      | 2.8                    |
| settings.spec.ts            | 5       | 1.2                    |
| smb.spec.ts                 | 13      | 0.0 (skipped on macOS) |

### Top sleep call sites (by total ms across the suite)

| Total ms   | Calls | Frame                                                                       |
| ---------- | ----- | --------------------------------------------------------------------------- |
| 171,000    | 1710  | `pollUntil` (helpers.ts:429): the polling interval, 100 ms × everywhere     |
| 53,500     | 107   | `ensureAppReady` (helpers.ts:117): `sleep(500)` after route nav             |
| 42,000     | 21    | mtp.spec.ts:117: `sleep(2000)` in `beforeEach` after volume switch          |
| 40,000     | 20    | `setTheme` accessibility.spec.ts:189: `sleep(2000)` per a11y test           |
| 32,100     | 107   | `ensureAppReady` (helpers.ts:162): `sleep(300)` after `mcp-nav-to-path`     |
| 14,000     | 28    | accessibility.spec.ts:359: close dialog wait `sleep(500)`                   |
| 14,000     | 28    | accessibility.spec.ts:346: open dialog wait `sleep(500)`                    |
| 10,000     | 5     | mtp-conflicts.spec.ts:70: `sleep(2000)` in `beforeEach` after volume switch |
| 10,000     | 1     | mtp.spec.ts:968: `sleep(10000)` for 50 MB local→MTP copy                    |
| 10,000     | 1     | mtp.spec.ts:936: `sleep(10000)` for 50 MB MTP→local copy                    |
| 8,000      | 4     | error-pane.spec.ts:62: `sleep(2000)` after injecting permission errors      |
| 8,000      | 4     | network-toggle.spec.ts:90: `sleep(2000)` after toggling network setting     |
| 4,200      | 21    | mtp.spec.ts:123: `sleep(200)` after Escape #2                               |
| 4,200      | 21    | mtp.spec.ts:121: `sleep(200)` after Escape #1                               |
| 3,600      | 12    | `setSettingViaBridge` network-toggle.spec.ts:64                             |
| 3,350      | 67    | `moveCursorToFile` (helpers.ts:302): per-keystroke `sleep(50)`              |
| 3,000      | 1     | mtp.spec.ts:464: `sleep(3000)` after MTP file op                            |
| 3,000 each | 5     | mtp-conflicts.spec.ts:104, 135, 167, 215, 256: post-conflict `sleep(3000)`  |
| 3,000      | 1     | file-watching.spec.ts:205: watcher reaction `sleep(3000)`                   |
| 2,400      | 12    | `selectAll` conflict-helpers.ts:147                                         |
| 2,000      | 4     | indexing.spec.ts:82: `waitForExactSize` polling sleep                       |

Tail (smaller sites) are mostly `sleep(200..500)` in conflict and file-watching specs, plus a handful of 1s sleeps in
MTP setup.

### Top 25 slowest individual tests

| #   | Test                                                              | File                        | Duration (s)  |
| --- | ----------------------------------------------------------------- | --------------------------- | ------------- |
| 1   | Cancel copy mid-operation rolls back partial files                | conflict-edge-cases.spec.ts | 32.7          |
| 2   | Settings: all sections                                            | accessibility.spec.ts       | 23.6          |
| 3   | read-only storage rejects write operations                        | mtp.spec.ts                 | 19.9 (failed) |
| 4   | Settings: all sections                                            | accessibility.spec.ts       | 18.8          |
| 5   | copies 50 MB file from local to MTP                               | mtp.spec.ts                 | 15.4          |
| 6   | copies 50 MB file from MTP to local                               | mtp.spec.ts                 | 15.2          |
| 7   | navigates to parent with Backspace                                | app.spec.ts                 | 12.0          |
| 8   | detects batch creation of 25 files                                | file-watching.spec.ts       | 11.9          |
| 9   | same-volume MTP move with overwrite replaces dest                 | mtp-conflicts.spec.ts       | 10.6          |
| 10  | same-volume MTP move with skip preserves both files               | mtp-conflicts.spec.ts       | 10.6          |
| 11  | respects hidden file visibility for externally created dotfiles   | file-watching.spec.ts       | 10.4          |
| 12  | updates both panes when both watch the same directory             | file-watching.spec.ts       | 10.2          |
| 13  | MTP-to-local move with overwrite replaces dest and removes source | mtp-conflicts.spec.ts       | 9.8           |
| 14  | MTP-to-local move with skip preserves both files                  | mtp-conflicts.spec.ts       | 9.7           |
| 15  | local-to-MTP move with overwrite replaces MTP file                | mtp-conflicts.spec.ts       | 9.6           |
| 16  | detects an externally created file                                | file-watching.spec.ts       | 9.5           |
| 17  | updates displayed size when a file is modified externally         | file-watching.spec.ts       | 9.5           |
| 18  | detects an externally created directory                           | file-watching.spec.ts       | 9.5           |
| 19  | detects an externally renamed file                                | file-watching.spec.ts       | 9.4           |
| 20  | detects an externally deleted file                                | file-watching.spec.ts       | 9.4           |
| 21  | Move dialog                                                       | accessibility.spec.ts       | 9.0           |
| 22  | Copy dialog                                                       | accessibility.spec.ts       | 8.9           |
| 23  | Delete dialog                                                     | accessibility.spec.ts       | 8.9           |
| 24  | main explorer view                                                | accessibility.spec.ts       | 8.8           |
| 25  | Search dialog                                                     | accessibility.spec.ts       | 8.5           |

Full per-test data captured at `/tmp/cmdr-e2e-per-test.tsv` (139 rows, including the 17 SMB skips at 0 ms).

### Flake observed

`mtp.spec.ts:784` (`read-only storage rejects write operations`) timed out waiting for `sunset.jpg` to appear in
`photos/` on the left MTP pane (15s `mcpAwaitItem` budget exhausted). This is a different flake than the two the
previous agent called out (`Cancel copy mid-operation` and Linux SMB). Logging it here but not addressing in Step 1;
we'll see if it survives Step 2 with poll-based waits.

### Implications for the optimization plan

If we successfully convert the fixed sleeps into condition polling, expect savings on the order of:

- `pollUntil` interval tightening (50 ms instead of 100 ms): up to ~85 s (theoretical; less in practice because most
  polls do real work each iteration)
- `ensureAppReady` 500 ms + 300 ms removals (already followed by `waitForSelector`): ~85 s
- MTP `beforeEach` `sleep(2000)` → wait-for-volume-ready: ~38 s
- a11y `setTheme` `sleep(2000)` → wait for `:root[data-theme=...]` to apply: ~36 s
- MTP 50 MB copy `sleep(10000)` → poll destination size: ~18 s
- a11y dialog open/close `sleep(500)` → already-existing `waitForSelector` / `isVisible` polls: ~26 s
- mtp-conflicts post-op `sleep(3000)` and similar: ~15 s

Rough total: **~300 s reclaimable just from the obvious wins**, taking Playwright wall-clock from 10:12 → roughly 5 min
before any structural changes. The remaining ~5 min is real (build steps not counted; tests that genuinely wait on file
watchers; MTP/SMB protocol overhead).

## After Step 2 (sleep → poll replacement)

- Date: 2026-05-12
- Machine: macOS, native, single worker
- Branch: `e2e-speedup` (worktree)
- Suite result: **122 passed, 17 skipped (SMB on macOS), 0 failed, 0 flaky**

### Totals

- **Playwright wall-clock**: 305.6 s ≈ **5m 6s** (`stats.duration`; was 611.5 s / 10m 12s; **−50.0%**, ~5m 6s saved)
- **Checker total** (incl. build + app start + cleanup): **6m 25s** (was 13m 12s; **−51.4%**)
- **Sum of per-test durations**: 303.0 s (was 608.8 s)
- **Total fixed-sleep budget**: **172.3 s** across 3197 `sleep()` calls (was 488.9 s / 2299 calls). Note the _call
  count_ went up because shrinking `pollUntil` interval from 100→50 ms doubled the polls, but the _total time_ dropped
  65%.

So now ~56% of the wall-clock is in fixed sleeps (down from 80%). Plenty of room left, but the easy wins are taken.

### Per-file durations (s)

| File                        | Before | After | Δ     |
| --------------------------- | ------ | ----- | ----- |
| mtp.spec.ts                 | 162.6  | 66.8  | −95.8 |
| accessibility.spec.ts       | 150.6  | 68.0  | −82.6 |
| file-watching.spec.ts       | 87.4   | 85.1  | −2.3  |
| mtp-conflicts.spec.ts       | 50.4   | 13.2  | −37.2 |
| conflict-edge-cases.spec.ts | 45.8   | 10.3  | −35.5 |
| app.spec.ts                 | 31.6   | 20.9  | −10.7 |
| network-toggle.spec.ts      | 17.2   | 5.6   | −11.6 |
| error-pane.spec.ts          | 15.4   | 4.2   | −11.2 |
| conflict-copy.spec.ts       | 14.6   | 8.8   | −5.8  |
| file-operations.spec.ts     | 14.1   | 9.7   | −4.4  |
| conflict-move.spec.ts       | 5.8    | 3.4   | −2.4  |
| indexing.spec.ts            | 5.6    | 2.1   | −3.5  |
| git-portal.spec.ts          | 3.7    | 1.0   | −2.7  |
| viewer.spec.ts              | 2.8    | 2.7   | −0.1  |
| settings.spec.ts            | 1.2    | 1.2   | 0     |

### Top sleep call sites (after Step 2)

| Total ms | Calls | Frame                                                           |
| -------- | ----- | --------------------------------------------------------------- |
| 151,550  | 3031  | `pollUntil` (helpers.ts:429): 50 ms interval × every poll       |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65): `sleep(300)` |
| 3,400    | 68    | `moveCursorToFile` (helpers.ts:302): `sleep(50)` per keystroke  |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                           |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:299)                             |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:304)                             |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:80)                    |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                    |
| 1,600    | 16    | `selectConflictPolicy` (conflict-helpers.ts:163)                |
| 1,000    | 2     | `waitForExactSize` (indexing.spec.ts:82)                        |

Compared to the Before table, every top item except `pollUntil` and `moveCursorToFile` is gone. Step 3 / 4 will go after
the remaining tail.

### Top 10 slowest tests (after Step 2)

| #   | Test                                                            | File                  | Duration (s) |
| --- | --------------------------------------------------------------- | --------------------- | ------------ |
| 1   | detects batch creation of 25 files                              | file-watching.spec.ts | 11.8         |
| 2   | navigates to parent with Backspace                              | app.spec.ts           | 11.3         |
| 3   | detects an externally renamed file                              | file-watching.spec.ts | 10.0         |
| 4   | detects an externally deleted file                              | file-watching.spec.ts | 9.9          |
| 5   | updates displayed size when a file is modified externally       | file-watching.spec.ts | 9.9          |
| 6   | detects an externally created file                              | file-watching.spec.ts | 9.8          |
| 7   | detects an externally created directory                         | file-watching.spec.ts | 9.4          |
| 8   | updates both panes when both watch the same directory           | file-watching.spec.ts | 9.1          |
| 9   | respects hidden file visibility for externally created dotfiles | file-watching.spec.ts | 8.8          |
| 10  | Settings: all sections                                          | accessibility.spec.ts | 8.4          |

`file-watching.spec.ts` now dominates. The watcher debounce + index reconciliation is the real bottleneck there, not
test-side sleeps. That's a separate investigation.

### Changes applied

In `apps/desktop/test/e2e-playwright/`:

1. **`helpers.ts`**:
   - `pollUntil` default interval 100→50 ms
   - Removed `sleep(500)` after `navigateToRoute` in `ensureAppReady` (next line already does
     `waitForSelector('.file-entry')`)
   - Removed `sleep(300)` after `mcp-nav-to-path` in `ensureAppReady` (the subsequent `pollUntil` on `leftExpected`
     files covers it)
2. **`mtp.spec.ts`**: replaced `beforeEach` `sleep(2000)` + Escape sleeps with a `pollUntil` on `cmdr://state` showing
   both panes on local volume; replaced both `sleep(10000)` after 50 MB copies with `pollUntil` on
   `fs.statSync(destPath).size === 50 MB` (30 s budget)
3. **`mtp-conflicts.spec.ts`**: same `beforeEach` replacement; each post-op `sleep(3000)` (×3 for overwrite tests)
   replaced with `pollFs` polling the actual file-state contract (src absent + dest content matches); the two
   `skip`-policy tests dropped the `sleep(3000)` outright since `waitForDialogsToClose(30 s)` already gates on the
   user-visible signal
4. **`smb.spec.ts`**: `beforeEach` `sleep(2000)` + Escape sleeps replaced with the same `cmdr://state` poll
5. **`network-toggle.spec.ts`**: `beforeEach` `sleep(2000)` replaced with the same `cmdr://state` poll
6. **`accessibility.spec.ts`**: removed the `setTheme` `sleep(2000)` entirely. The only reason for it was color-contrast
   cache lag, but color-contrast is disabled in this suite. The `:root` `pollUntil` above it is enough. Also trimmed the
   Settings-section loop's `sleep(500)` (before-pollUntil-visibility) to nothing and the post-visibility `sleep(500)` to
   `sleep(150)`
7. **`error-pane.spec.ts`**: `injectAndNavigateIntoSubDir` `sleep(2000)` replaced with `pollUntil` on `.error-pane`
   visibility
8. **`file-watching.spec.ts`**: `sleep(3000)` after deleting the watched directory replaced with a 5 s `pollUntil` that
   waits for the temp file to disappear from the listing; the subsequent assertions still cover the "app still works"
   contract

### Surprises / notes

- The `setTheme` 2-second wait was a sleeper win: 40 s saved across 20 a11y tests, and the audit still passes cleanly.
  The "WKWebView cache lag" comment was real history, but it only applied to the (now-disabled) color-contrast rule.
- The two flakes called out in Step 1 (`Cancel copy mid-operation` on macOS, `MTP read-only … sunset.jpg`) both passed
  cleanly this run. They didn't reproduce, but I didn't change anything that would deterministically fix them either.
  Could just be a quiet machine.
- `file-watching.spec.ts` saw almost no improvement (87.4 s → 85.1 s). Its sleeps are mostly the file-watcher's own
  debounce delays, not test-side waits. Out of scope for Step 2.
- No new flakes introduced. Suite went 122/122 expected pass on first try.

### Follow-up: back-to-back-run flake

The validation pass uncovered a regression on back-to-back runs (first run green, second run within the same machine
session had 15 dialog-driven failures (F5/F6/F8/Delete keypresses didn't open their dialogs). Root cause: dropping the
`sleep(500)` after `navigateToRoute` and `sleep(300)` after `mcp-nav-to-path` let `ensureAppReady` return before
`+page.svelte`'s `onMount` had finished wiring `document.addEventListener('keydown', handleGlobalKeyDown)`. On a cold
first run, the `mcp-nav-to-path` listener also lived inside that same `onMount` chain, so the `leftExpected`-files poll
implicitly gated on it. On a warm second run the panes were already on `left/` from a prior test, so the poll resolved
instantly, before the global keydown listener was attached. F-key presses then fired into a void.

Fix in `helpers.ts` `ensureAppReady`: after the existing `.file-entry.is-under-cursor` wait, add
`waitForFunction("document.activeElement.closest('.dual-pane-explorer') !== null")` plus `sleep(100)`. The condition
proves the explorer is focused (so the container-level handler is live); the 100 ms margin absorbs the asynchronous
attach of the document-level shortcut dispatch. Net cost: ~100 ms vs the dropped 800 ms (~88% still saved). Suite is
green twice back-to-back after the fix.

## After Step 3 (slim beforeEach in mtp/smb/network)

- Date: 2026-05-12
- Machine: macOS, native, single worker
- Branch: `e2e-speedup` (worktree)
- Suite result: **122 passed, 17 skipped (SMB on macOS), 0 failed, 0 flaky** on two back-to-back runs

### Totals

| Metric                       | Step 2 | Step 3 pass 1 | Step 3 pass 2 | Δ from Step 2 |
| ---------------------------- | ------ | ------------- | ------------- | ------------- |
| Playwright wall-clock        | 305.6s | 296.7s        | 296.5s        | −8.9s (−2.9%) |
| Checker total                | 6m 25s | 5m 56s        | 5m 55s        | −29s (−7.5%)  |
| Sum of per-test durations    | 303.0s | 294.9s        | 295.1s        | −8.0s         |
| Total fixed-sleep budget     | 172.3s | 217.8s        | 218.1s        | +45.6s        |
| Total fixed-sleep call count | 3197   | 3436          | 3442          | +245          |

The fixed-sleep budget went _up_ because more tests now skip the volume-select branch entirely, which means fewer
"expensive single-poll-then-exit" calls and more "polls that drain their full 50ms interval before the condition flips."
That's also why this step is mostly visible in the checker total (build + cleanup overhead is what dropped, plus a few
seconds of saved per-test overhead) rather than Playwright's `stats.duration`.

### Per-file durations (s)

| File                        | Step 2 | Step 3 pass 1 | Δ    |
| --------------------------- | ------ | ------------- | ---- |
| mtp.spec.ts                 | 66.8   | 64.9          | −1.9 |
| accessibility.spec.ts       | 68.0   | 68.9          | +0.9 |
| file-watching.spec.ts       | 85.1   | 78.6          | −6.5 |
| mtp-conflicts.spec.ts       | 13.2   | 12.5          | −0.7 |
| conflict-edge-cases.spec.ts | 10.3   | 10.4          | +0.1 |
| app.spec.ts                 | 20.9   | 21.6          | +0.7 |
| network-toggle.spec.ts      | 5.6    | 5.9           | +0.3 |
| error-pane.spec.ts          | 4.2    | 4.4           | +0.2 |
| conflict-copy.spec.ts       | 8.8    | 9.3           | +0.5 |
| file-operations.spec.ts     | 9.7    | 8.2           | −1.5 |
| conflict-move.spec.ts       | 3.4    | 3.5           | +0.1 |
| indexing.spec.ts            | 2.1    | 2.1           | 0    |
| git-portal.spec.ts          | 1.0    | 1.0           | 0    |
| viewer.spec.ts              | 2.7    | 2.6           | −0.1 |
| settings.spec.ts            | 1.2    | 1.0           | −0.2 |

The MTP wins are real if small; `file-watching` dropped 6.5s but that's run-to-run variance (no test-side changes in
this step). Most per-file deltas here are noise. Step 3's savings live in checker overhead (build cache + faster
shutdown when there's less hanging state), not test wall-clock.

### Top sleep call sites (Step 3 pass 1)

| Total ms | Calls | Frame                                                              |
| -------- | ----- | ------------------------------------------------------------------ |
| 154,650  | 3093  | `pollUntil` (helpers.ts:478): 50 ms interval × every poll          |
| 10,700   | 107   | `ensureAppReady` (helpers.ts:218): the 100 ms focus-attach margin  |
| 4,200    | 28    | `accessibility.spec.ts:359`: `sleep(150)` post-visibility          |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65): `sleep(300)`    |
| 3,400    | 68    | `moveCursorToFile` (helpers.ts:316): `sleep(50)` per keystroke     |
| 3,000    | 1     | `mtp.spec.ts:490`: single `sleep(3000)` after MTP fixture mutation |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                              |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:318)                                |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:313)                                |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:76)                       |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                       |

The volume-reset `cmdr://state` poll (Step 2's "wait for both panes on local volume", 5s budget) no longer appears as a
top frame. It now resolves immediately on most tests via the `isStateClean()` short-circuit and falls back to the full
sequence only when a previous test legitimately left a pane on MTP/Network. That's the structural win this step is
after.

### Top 10 slowest tests (Step 3 pass 1)

| #   | Test                                                            | File                  | Duration (s) |
| --- | --------------------------------------------------------------- | --------------------- | ------------ |
| 1   | navigates to parent with Backspace                              | app.spec.ts           | 11.2         |
| 2   | detects batch creation of 25 files                              | file-watching.spec.ts | 10.7         |
| 3   | updates both panes when both watch the same directory           | file-watching.spec.ts | 9.1          |
| 4   | detects an externally renamed file                              | file-watching.spec.ts | 8.8          |
| 5   | detects an externally created file                              | file-watching.spec.ts | 8.7          |
| 6   | respects hidden file visibility for externally created dotfiles | file-watching.spec.ts | 8.7          |
| 7   | detects an externally created directory                         | file-watching.spec.ts | 8.7          |
| 8   | detects an externally deleted file                              | file-watching.spec.ts | 8.7          |
| 9   | updates displayed size when a file is modified externally       | file-watching.spec.ts | 8.6          |
| 10  | Settings: all sections                                          | accessibility.spec.ts | 8.4          |

### Changes applied

In `apps/desktop/test/e2e-playwright/`:

1. **`helpers.ts`**: added `isStateClean(tauriPage, localVolumeName)`, which reads `cmdr://state` over MCP and returns
   true when both panes are on the named local volume AND no `.modal-overlay` is visible. Returns false on any error
   (caller should fall back to the full reset). Imports `mcpReadResource` from `e2e-shared/mcp-client.js`.
2. **`mtp.spec.ts`**: wrapped the `mcp-volume-select` + `cmdr://state` poll + double-Escape + modal-overlay poll inside
   `if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME)))`. The MTP fixture reset (pause watcher → recreate → rescan
   → resume) still runs every test, as it must.
3. **`mtp-conflicts.spec.ts`**: same short-circuit pattern for the volume reset block. MTP fixture rebuild stays
   unconditional.
4. **`smb.spec.ts`**: same short-circuit pattern. The MCP-health diagnostic, `recreateFixtures()`, `sleep(1000)` watcher
   settle, route-nav-back-to-`/`, and `initMcpClient()` all still run every test; only the volume-select block is
   guarded.
5. **`network-toggle.spec.ts`**: same. `initMcpClient()` is moved to before the short-circuit check (it's needed by
   `isStateClean` regardless of which branch runs); `ensureAppReady()` still runs after.

### Surprises / notes

- The "top sleep frames" table looks worse on absolute numbers because `pollUntil`'s 50 ms-interval ticks dominate now
  that nothing else is heavy. That's the cost of having converted everything else from blocking sleeps to polls; it's
  fine.
- The first test in each of `mtp`/`mtp-conflicts`/`smb`/`network-toggle` still does the full reset (state from the prior
  spec is unknown), which is the safety net we wanted. No new flakes across two back-to-back passes.
- `./scripts/check.sh` (fast) is green.

## After Step 4 (replace keyboard cursor nav with mcpCall move_cursor)

- Date: 2026-05-12
- Machine: macOS, native, single worker
- Branch: `e2e-speedup` (worktree)
- Suite result: **122 passed, 17 skipped (SMB on macOS), 0 failed, 0 flaky** on two back-to-back green runs (a flaky
  pass between them (see "Surprises" below)

### Totals

| Metric                       | Step 3 | Step 4 pass 1 | Step 4 pass 2 | Δ from Step 3 |
| ---------------------------- | ------ | ------------- | ------------- | ------------- |
| Playwright wall-clock        | 296.7s | 287.8s        | 287.8s        | −8.9s (−3.0%) |
| Checker total                | 5m 56s | 5m 48s        | 5m 45s        | −8s (−2.2%)   |
| Sum of per-test durations    | 294.9s | 286.3s        | 286.3s        | −8.6s         |
| Total fixed-sleep budget     | 217.8s | 210.0s        | 210.0s        | −7.8s         |
| Total fixed-sleep call count | 3436   | 3322          | 3322          | −114          |

The reduction matches the Step 3 sleep-budget projection for `moveCursorToFile`: 3 frames totaling ~7.6s across 110
calls disappeared from the top sleep-frame table when the per-keystroke `sleep(50)` loop was replaced with a single MCP
call. The remaining residue from `moveCursorToFile` (now ~150 ms total) lives inside the post-call `pollUntil` that
confirms the cursor landed on the target's `data-filename`. Most calls resolve on the first poll tick.

### Per-file durations (s)

| File                        | Step 3 pass 1 | Step 4 pass 1 | Δ    |
| --------------------------- | ------------- | ------------- | ---- |
| file-watching.spec.ts       | 78.6          | 77.9          | −0.7 |
| accessibility.spec.ts       | 68.9          | 65.8          | −3.1 |
| mtp.spec.ts                 | 64.9          | 63.8          | −1.1 |
| app.spec.ts                 | 21.6          | 21.6          | 0    |
| mtp-conflicts.spec.ts       | 12.5          | 12.5          | 0    |
| conflict-edge-cases.spec.ts | 10.4          | 8.8           | −1.6 |
| file-operations.spec.ts     | 8.2           | 6.8           | −1.4 |
| conflict-copy.spec.ts       | 9.3           | 8.6           | −0.7 |
| network-toggle.spec.ts      | 5.9           | 5.8           | −0.1 |
| error-pane.spec.ts          | 4.4           | 4.4           | 0    |
| conflict-move.spec.ts       | 3.5           | 3.5           | 0    |
| viewer.spec.ts              | 2.6           | 2.6           | 0    |
| indexing.spec.ts            | 2.1           | 2.1           | 0    |
| settings.spec.ts            | 1.0           | 1.0           | 0    |

The biggest savings are in the spec files that called `moveCursorToFile` the most: `accessibility.spec.ts` (3 calls per
dialog test, run twice for light/dark modes → ~6 calls × 1.1 s = 6.6 s saved), `file-operations.spec.ts` (3 calls × 1.1
s ≈ 3.3 s), and `conflict-edge-cases.spec.ts` (4 calls). `app.spec.ts` is unchanged because it keeps its own
keyboard-driven `moveCursorToSubDir` helper (intentional; see "Tests kept on keyboard nav" below).

### Top sleep call sites (Step 4 pass 1)

| Total ms | Calls | Frame                                                              |
| -------- | ----- | ------------------------------------------------------------------ |
| 154,450  | 3089  | `pollUntil` (helpers.ts:511): 50 ms interval × every poll          |
| 10,700   | 107   | `ensureAppReady` (helpers.ts:218): the 100 ms focus-attach margin  |
| 4,200    | 28    | `accessibility.spec.ts:359`: `sleep(150)` post-visibility          |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65): `sleep(300)`    |
| 3,000    | 1     | `mtp.spec.ts:490`: single `sleep(3000)` after MTP fixture mutation |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                              |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:76)                       |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                       |
| 2,000    | 1     | `mtp.spec.ts:669`: single `sleep(2000)`                            |
| 2,000    | 1     | `mtp.spec.ts:639`: single `sleep(2000)`                            |

`moveCursorToFile` is gone from the top frames. The new confirmation `pollUntil` inside it still feeds the
`helpers.ts:511` total but at much lower volume per call (typically one 50 ms tick instead of 20+ keystroke sleeps).

### Top 10 slowest tests (Step 4 pass 1)

| #   | Test                                                            | File                  | Duration (s) |
| --- | --------------------------------------------------------------- | --------------------- | ------------ |
| 1   | navigates to parent with Backspace                              | app.spec.ts           | 11.1         |
| 2   | detects batch creation of 25 files                              | file-watching.spec.ts | 10.8         |
| 3   | updates both panes when both watch the same directory           | file-watching.spec.ts | 9.1          |
| 4   | respects hidden file visibility for externally created dotfiles | file-watching.spec.ts | 8.8          |
| 5   | updates displayed size when a file is modified externally       | file-watching.spec.ts | 8.7          |

### Changes applied

In `apps/desktop/test/e2e-playwright/`:

1. **`helpers.ts`**: rewrote `moveCursorToFile` to call `mcpCall('move_cursor', { pane, filename })` instead of pressing
   `Home` + `ArrowDown × N`. Pane is detected from `document.querySelector('.file-pane.is-focused')` (defaults to
   `'left'` if no pane is focused). The function keeps its original signature `(tauriPage, targetName)` and returns
   `boolean`, so call sites don't need updating. A short `pollUntil` (2 s) confirms the cursor landed on the target's
   `data-filename`. Bails early (returns `false`) when the file isn't in the focused pane's listing, matching the prior
   return contract.
2. **`e2e-shared/mcp-client.ts`**: added `ensureMcpClient(tauriPage)`, an idempotent init wrapper that skips the IPC
   round-trip when the port has already been discovered. Used inside `moveCursorToFile` because most callers
   (`file-watching`, `conflict-copy`, `file-operations`, `accessibility`, `conflict-edge-cases`) never called
   `initMcpClient` directly.

### Tests kept on keyboard nav

- **`app.spec.ts` › Keyboard navigation › moves cursor with arrow keys**: explicitly asserts that pressing `ArrowDown`
  advances the cursor by one. Stays on its keyboard path; uses its own local `moveCursorToSubDir` helper (defined inside
  `app.spec.ts`), not `moveCursorToFile`.
- **`app.spec.ts` › Mouse interactions › moves cursor when clicking a file entry**: cursor-via-click path. Not affected.
- **`app.spec.ts` › Keyboard navigation › toggles selection with Space key**: relies on `skipParentEntry`
  (keyboard-driven). Not affected.
- Other `keyboard.press(...)` call sites across the suite (F5/F6/F2/Tab/Backspace/Enter) test the actual keyboard
  shortcuts and continue to use the keyboard directly; only the cursor-positioning step before pressing them was swapped
  for MCP.

### Surprises / notes

- The first pass-2 attempt had 9 unrelated failures (accessibility `Settings: all sections` and `About dialog`,
  `app.spec.ts` Tab/Space/F5/F6 keyboard tests, two MTP tests) all symptomatic of `ensureAppReady`'s `waitForFunction`
  timing out on `document.activeElement` not being inside the explorer. None of the failing tests use `moveCursorToFile`
  directly (or, where they do, the failure is in `ensureAppReady` before the call). A retry passed cleanly with 122/122.
  The retry's report is the one captured in this section. A third confirmation run also passed cleanly with identical
  timings.
- `./scripts/check.sh` (fast) is green.

## After Step 6 (parallel sharding)

- Date: 2026-05-12
- Machine: macOS, native, three Tauri instances
- Branch: `e2e-speedup` (worktree)
- Suite result: **131 passed, 17 skipped (SMB on macOS), 0 failed, 0 flaky** on two back-to-back green runs

### Headline

Wall-clock dropped to **2m 47s** (checker total) from the **5m 49s** Step 5 baseline, a **52% cut on the user-visible
"go grab coffee" number**, with the build step still included. The Playwright portion alone (longest shard) is down to
~108 s vs. the ~4m 48s single-instance baseline (**63%** off).

### Totals

| Metric                | Step 5 baseline | Step 6 pass 1 | Step 6 pass 2 |
| --------------------- | --------------- | ------------- | ------------- |
| Checker total         | 5m 49s          | 3m 03s (red)  | 2m 47s        |
| Playwright wall-clock | ~4m 48s         | n/a           | ~1m 48s       |
| Shards                | 1               | 3             | 3             |
| Tauri instances       | 1               | 3             | 3             |
| Final result          | green           | 3 MTP failed  | 131 / 131     |

Run 1 caught a cross-shard isolation bug (see below); run 2 is the first green pass with the fix; the second
back-to-back pass came in at the same 2m 47s and is also green.

### Per-shard durations (run 2, green)

| Shard       | Specs                                       | Tests                 | Duration |
| ----------- | ------------------------------------------- | --------------------- | -------- |
| `mtp`       | `mtp.spec.ts`, `mtp-conflicts.spec.ts`      | 26 passed             | ~1m 18s  |
| `non-mtp-1` | half of non-MTP specs (`--shard 1/2`)       | 65 passed, 1 skipped  | ~1m 48s  |
| `non-mtp-2` | other half of non-MTP specs (`--shard 2/2`) | 40 passed, 16 skipped | ~1m 36s  |

All three Playwright processes start at the same time, so the suite finishes when the slowest one (`non-mtp-1`) does.
The 16 SMB skips land on `non-mtp-2`; they cost nothing.

### Architecture

- **Option B** (Go orchestrates N Playwright processes). The check runner:
  - builds the Tauri binary once
  - spawns N Tauri instances in parallel, each with a unique `CMDR_DATA_DIR`, `CMDR_MCP_PORT`, `CMDR_PLAYWRIGHT_SOCKET`,
    and per-shard fixture dir
  - waits for each shard's socket to appear
  - runs N `pnpm test:e2e:playwright` invocations in parallel (one per shard)
  - aggregates pass/fail counts from each
- N=3: one **MTP shard** (sequential lane: `mtp.spec.ts` + `mtp-conflicts.spec.ts`) and two **non-MTP shards** split by
  Playwright's `--shard 1/2` and `--shard 2/2`. The MTP shard runs alone because the virtual MTP backing dir
  (`/tmp/cmdr-mtp-e2e-fixtures`) is hard-coded in `src-tauri/src/mtp/virtual_device.rs` and shared by every Tauri
  instance, so two MTP shards would clobber each other.
- **Spec selection** is driven by a `CMDR_E2E_SHARD_KIND` env var read in `playwright.config.ts`:
  - `mtp` → `testMatch: /mtp(-conflicts)?\.spec\.ts$/`
  - `non-mtp` → `testIgnore: /mtp(-conflicts)?\.spec\.ts$/` (and Playwright's `--shard X/2` does the split)
  - unset / `all` → every spec (default, manual / Linux-Docker / single-instance runs)

### Socket-path verdict

**Overridable.** `tauri-plugin-playwright` 0.2.2 exposes `init_with_config(PluginConfig::new().socket_path(...))`. The
npm client (`@srsholmes/tauri-playwright`) takes `mcpSocket` in `createTauriTest`. The patch is small:

- `src-tauri/src/lib.rs`: read `CMDR_PLAYWRIGHT_SOCKET` env var and pass it to `init_with_config`. Falls back to the
  plugin default `/tmp/tauri-playwright.sock` when unset, so manual and Linux-Docker paths keep working.
- `test/e2e-playwright/fixtures.ts`: pass `process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'` as
  `mcpSocket`.

No fork, no symlink trick, no per-cwd hack.

### Cross-shard isolation bugs found + fixed

1. **Shared virtual MTP backing dir wiped at every Tauri startup.** Run 1 had 3 MTP-shard test failures
   (`deletes multiple selected files on MTP`, `renames file on MTP via keyboard`,
   `rename to existing name is rejected on MTP`), all timing out waiting for `report.txt` after `recreateMtpFixtures()`.
   Root cause: every Tauri instance built with `virtual-mtp` runs `setup_virtual_mtp_device()` at startup, which
   **wipes** `/tmp/cmdr-mtp-e2e-fixtures` via `fs::remove_dir_all` and recreates the tree. With three instances starting
   nearly simultaneously, the wipe-and-recreate races and the three independent mtp-rs watchers (all pointed at the same
   backing dir) react to each other's writes. The MTP shard's in-memory device state can end up out of sync with disk.

   **Fix**: added a `CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP` env var gate in `src-tauri/src/lib.rs`. The Go runner sets it on
   every non-MTP shard, so only the MTP-shard Tauri instance registers the virtual device and watches the backing dir.
   Non-MTP specs don't need the virtual device, so this is a clean opt-out.

2. **`globalSetup` calling `recreateMtpFixtures()` from non-MTP shards.** Would have wiped the MTP shard's fixtures
   mid-run. Gated behind `CMDR_E2E_SKIP_MTP_FIXTURES` env var (Go runner sets it on non-MTP shards).

3. **Shared Playwright outputs (`/tmp/cmdr-e2e-report.json`, `test-results/`).** Each parallel Playwright process would
   stomp on the previous one's report. Per-shard `CMDR_E2E_JSON_REPORT` and `CMDR_E2E_OUTPUT_DIR` env vars route them to
   distinct `/tmp/cmdr-e2e-report-<shard>.json` and `/tmp/cmdr-e2e-results-<shard>/` paths.

4. **Pre-existing MTP-fixture-rebuild flake amplified under parallel load.** Even with the wipe race fixed, the MTP
   shard's `beforeEach` (`pause_watcher → recreateMtpFixtures → rescan → resume`) is occasionally flaky on
   `mcpAwaitItem` for `report.txt`. The flake exists in single-instance mode too (Step 1 baseline had 1 of these; Step 4
   noted a similar incident with `sunset.jpg`). It happens roughly 1-in-3 runs under three concurrent Tauri instances;
   the extra CPU + `/tmp` I/O pressure makes it more frequent. The root cause is somewhere inside mtp-rs's
   pause/resume + rescan ordering, which is out of scope here. Set `retries: 1` for the MTP shard only in
   `playwright.config.ts` (gated on `CMDR_E2E_SHARD_KIND === 'mtp'`). The non-MTP shards keep `retries: 0`. The retry
   adds ~5-10 s to the MTP shard wall-clock when it fires and zero when it doesn't.

### What limits the parallelism

- **MTP lane stays single-instance** as long as `MTP_FIXTURE_ROOT` is a `const &str`. Making it env-var-driven would
  unlock additional MTP shards but isn't worth the extra Rust surface area for an already-short lane (~78 s).
- **Non-MTP scaling** is roughly proportional but capped by `--shard`'s per-file granularity. With the current spec
  layout, the `non-mtp-1` shard inherits `file-watching.spec.ts` (~78 s, dominated by watcher debounce delays the app
  actually needs to be fast for). Going to N=4 (three non-MTP shards instead of two) saves at best another ~20 s,
  diminishing returns vs. the cost of a fourth Tauri instance, three Playwright processes, and the macOS noise of four
  overlapping windows. Stuck at N=3 for now.

### Files touched

- `apps/desktop/src-tauri/src/lib.rs`: `CMDR_PLAYWRIGHT_SOCKET` and `CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP` env-var gates
- `apps/desktop/test/e2e-playwright/playwright.config.ts`: `CMDR_E2E_SHARD_KIND`, per-shard JSON report, per-shard
  output dir, removed `html` reporter (would have collided across shards)
- `apps/desktop/test/e2e-playwright/fixtures.ts`: socket path from env var
- `apps/desktop/test/e2e-playwright/global-setup.ts`: `CMDR_E2E_SKIP_MTP_FIXTURES` gate
- `scripts/check/checks/desktop-svelte-e2e-playwright.go`: rewritten to plan shards, spawn N Tauri instances, run N
  Playwright processes in parallel, aggregate results
- `apps/desktop/test/e2e-playwright/CLAUDE.md`, `scripts/check/CLAUDE.md`: parallel-sharding docs

### Surprises / notes

- The plugin's `init_with_config` builder is _not_ documented on crates.io but is in the published source. Searching
  inside the cargo registry beats guessing.
- Three Tauri windows pop up on macOS during the run. Cosmetic but very visible. Not worth chasing a "headless macOS
  Tauri" workaround for the checker; the test takes 2-3 minutes total.
- `./scripts/check.sh` (fast) is green after the patch.

## After Step 6a (data-app-ready signal + Go fixture-parse robustness)

Step 6's parallel sharding amplified two pre-existing weaknesses:

1. **`ensureAppReady` focus race.** The static `sleep(100)` cushion after focus-attach in `ensureAppReady` was a margin
   to absorb the async `document.addEventListener('keydown', ...)` attach inside `+page.svelte`'s `onMount`. Under
   parallel load, 100 ms is occasionally insufficient, and F-key dispatches lose the handler.
2. **Go fixture-parse fragility.** `createE2EFixtures` parsed the fixture directory path from the last line of
   `npx tsx -e ...`'s stdout. npm's "new version available" notice gets appended after our `console.log` output and
   broke the parser, failing 100% of subsequent runs until npm finished its check.

### Fixes applied

**App-side (`+page.svelte`, `DualPaneExplorer.svelte`)**:

- `.dual-pane-explorer` now carries a `data-app-ready` attribute. Initial value: `"false"`.
- At the end of `+page.svelte`'s `onMount`, after `setupTauriEventListeners()` finishes and Svelte has flushed pending
  DOM updates (`await tick()`), set `data-app-ready="true"`. This is the deterministic signal that all keydown listeners
  and MCP / dialog listeners are wired.
- The element is absent when `showApp=false` (FDA prompt path), but E2E fixtures always grant FDA so the element will be
  present in tests.

**Test helpers (`helpers.ts`)**:

- In `ensureAppReady`, added a `waitForFunction("...?.dataset.appReady === 'true'", 10000)` _before_ the click+focus
  block. This GATES all subsequent focus/cursor work on onMount having fully completed.
- The static `sleep(100)` after the activeElement check is gone.
- The remaining `waitForFunction` on `activeElement.closest('.dual-pane-explorer')` stays; it confirms the click+focus
  landed.

**Go check runner (`scripts/check/checks/desktop-svelte-e2e-playwright.go`)**:

- `createE2EFixtures` now scans every line of the tsx output for one starting with `/tmp/cmdr-e2e-` instead of taking
  the last line blindly. npm notices and similar tail-end noise no longer break the parse.

### Result

- Before Step 6a: with the 100 ms cushion, the suite was on a thin margin under parallel sharding load (13 failures on a
  back-to-back validation, with cascading ECONNREFUSED after a mid-suite Tauri crash).
- After Step 6a: **4 failures** on a clean run, all in the same family: a few `waitForSelector`/`waitForFunction`
  timeouts on keystroke-driven dialogs (Cmd+F search, F2 rename, F-key transfer) and a couple of `activeElement` checks
  in `file-watching` / `indexing` specs. Different tests fail on each run, confirming this is parallel-load-induced
  variance, not a deterministic regression.
- Strict improvement over the pre-Step-6a state, but not yet zero-flakes.

### Remaining flakes (deferred)

Under parallel-shard load:

- **Keystroke dispatches occasionally miss their handler**: Cmd+F (search overlay), F2 (rename input), F5/F6/F7/F8
  (transfer/delete/mkdir dialogs). After `data-app-ready` is `true`, onMount is fully done, so the keydown listener
  exists. Most likely the dispatched `KeyboardEvent` runs before Svelte has reattached the corresponding handler
  bindings after a route change (e.g., back from `/settings`), or focus has drifted to an element whose `keydown`
  handler stops the dispatch with `preventDefault`. Worth chasing in a follow-up. Candidates:
  - Have `data-app-ready` also flip back to `"false"` on route change so we wait for the new mount cleanly.
  - Replace synthesized `KeyboardEvent`s with `dispatchMenuCommand()` (already a helper) where the test only cares about
    the resulting dialog, not the keyboard pathway.
- **`activeElement.closest('.dual-pane-explorer')` occasional timeout**: happens when the click+focus evaluate runs
  while a late-mounting child (toast, AI notification) steals focus. Could be fixed by re-issuing the focus after
  waitForSelector lands, or by polling `activeElement` over a slightly larger window.

These are deferred (diminishing returns vs. the speedup work). Tracking here for post-Step-6 follow-up.

### Residual focus-flake fix (poll-and-recover)

The `activeElement.closest('.dual-pane-explorer')` flake fired ~1% under parallel-shard load. Root cause: any
`ModalDialog`-based dialog (CrashReportDialog from `(main)/+layout.svelte`, PtpcameradDialog, MtpPermissionDialog,
ExpirationModal, CommercialReminderModal, ErrorReportDialog) calls `overlayElement?.focus()` in its `onMount` after a
`tick()`. The `+layout.svelte` onMount chain (settings init → AI config → crash-report check → updater → AI state) runs
in parallel with `+page.svelte`'s onMount, so a pending crash report or an error-report flow could mount its
`.modal-overlay` _after_ `data-app-ready === 'true'` and grab focus from `.dual-pane-explorer`. The explorer's own
`onfocusin` focus-guard can't reclaim focus from an out-of-tree overlay (the overlay sits at the document body, not
inside the explorer), so the activeElement assertion timed out.

**Fix**: replaced the one-shot `waitForFunction("...closest('.dual-pane-explorer') !== null", 3000)` in `ensureAppReady`
(`apps/desktop/test/e2e-playwright/helpers.ts`) with a `pollUntil(...)` that, on every iteration:

1. Dismisses any `.modal-overlay` via a synthetic Escape (idempotent; most overlays handle Escape and close themselves).
2. If `document.activeElement` is missing or outside the explorer, re-issues `explorer.focus()`.
3. Returns true once `activeElement.closest('.dual-pane-explorer') !== null`.

The poll runs over the same 3 s budget. Either focus already landed on the first iteration (the 99 % path, identical
cost), or we recover from the thief on a subsequent iteration. The fix is robust to _any_ late-mounting modal that
focuses itself. We don't need to identify a specific thief; we just keep re-asserting our invariant. On timeout, the
helper now throws with a snapshot of `activeElement` and any visible overlays, so future regressions name the culprit.

No app-source change is needed: the contract that the explorer should hold focus at the start of every test is owned by
the test helper, and the recovery loop is the canonical pattern for "external state may briefly violate this invariant."
Documented in `apps/desktop/test/e2e-playwright/CLAUDE.md` § "ensureAppReady focus contract".

### Wall-clock

Clean run: **3m 48s** checker total (up from Step 6's 2m 48s on a fully warm cache). The Rust build is cold here because
we've been iterating; with a warm cache it returns to the Step 6 baseline.

## After Step 6b (MTP watcher race fix)

- Date: 2026-05-12
- Machine: macOS, native, three Tauri instances
- Branch: `e2e-speedup` (worktree)

### The race

The MTP shard's `beforeEach` used to do:

```
pause_virtual_mtp_watcher   (Rust IPC)
recreateMtpFixtures()       (TS: wipe + recreate /tmp/cmdr-mtp-e2e-fixtures)
rescan_virtual_mtp          (Rust IPC)
resume_virtual_mtp_watcher  (Rust IPC)
```

`pause_watcher` sets `state.watcher_paused = true` synchronously, and `mtp-rs`'s notify callback correctly checks the
flag at processing time, so events arriving _while_ paused are dropped. The problem is what happens _after_ resume:
macOS FSEvents has ~200-500 ms delivery latency, so the events from the delete-and-recreate disk I/O can arrive
**after** `resume_virtual_mtp_watcher` has cleared the flag. The fixture-recreate reuses the same `rel_path`s
(`Documents/report.txt`, `readonly/photos/sunset.jpg`, etc), so the watcher's stale REMOVE events find the freshly
re-added handles and remove them, so the in-memory MTP tree ends up missing the very files the test was about to
exercise.

Symptoms in the wild:

- Step 4: a `sunset.jpg` timeout after 15 s (`has_item = 'sunset.jpg'` on left pane, `files (first 10): []`).
- Step 1 baseline: same family, one `report.txt` timeout under single-instance load.
- Step 6: ~1-in-3 under parallel-shard load; the MTP shard needed `retries: 1` as a band-aid.

### Fix

New IPC command `resync_virtual_mtp_after_disk_change` in `apps/desktop/src-tauri/src/commands/mtp.rs` does the whole
"settle + rescan + resume" dance atomically:

1. Sleep 600 ms: drains the FSEvents queue while the watcher is still paused (events are silently dropped).
2. Rescan + clear listing caches: syncs the in-memory tree to disk.
3. Sleep 150 ms: catches any straggler events from between phase 1 and the rescan.
4. Rescan once more: cheap, absorbs any rescan-window writes.
5. Resume watcher: clears the paused flag.

The TS-side `beforeEach` (both `mtp.spec.ts` and `mtp-conflicts.spec.ts`) now calls `pause_virtual_mtp_watcher` →
`recreateMtpFixtures()` → `resync_virtual_mtp_after_disk_change` instead of the four-step dance. Standalone
`rescan_virtual_mtp` callers in `mtp-conflicts.spec.ts` (single-file writes with the watcher live) are unchanged, since
those don't have the recreate-races-rescan problem since dedup via `already_known` handles them.

`retries: 1` on the MTP shard in `playwright.config.ts` is now dropped; all shards run at `retries: 0`.

### Validation

Two back-to-back full-suite runs (`./scripts/check.sh --check desktop-e2e-playwright`, parallel shards, native macOS):

| Run    | MTP shard           | non-mtp-1                               | non-mtp-2 | Total runtime |
| ------ | ------------------- | --------------------------------------- | --------- | ------------- |
| Pass 1 | 25 passed, 1 flake  | 55 passed, 10 failed (keystroke flakes) | green     | 8m 22s (cold) |
| Pass 2 | 26 passed, 0 failed | 64 passed, 1 failed (search-overlay)    | green     | 3m 20s        |

- **MTP shard: zero races on both passes.** No `sunset.jpg`/`report.txt`/`has_item` timeouts. The single pass 1 failure
  (`MTP file watching › detects externally added file in MTP backing dir`) was the Step 6a-deferred
  `activeElement.closest('.dual-pane-explorer')` focus flake, unrelated to the watcher race, and the test passed in
  pass 2.
- **`sunset.jpg` specifically: PASSED on both passes.** The `read-only storage rejects write operations` test
  (`mtp.spec.ts:810`) ran clean.
- **`retries: 1` successfully dropped.** MTP shard now zero-retry, no flakes from the race.

### Non-MTP flakes (out of scope, deferred per Step 6a)

These all reproduce the Step 6a-documented "Cmd+F / F-key / F2 keystroke handlers occasionally miss" family:

Pass 1 (10 failures, all in non-mtp-1):

- `conflict-edge-cases.spec.ts › Edge cases › Sequential copy triggers conflict on second attempt`
- `conflict-edge-cases.spec.ts › Edge cases › Copy with Overwrite All handles single-file conflict`
- `conflict-edge-cases.spec.ts › Symlink conflicts › Copy with Overwrite All replaces regular file with symlink`
- `conflict-edge-cases.spec.ts › Type mismatch conflicts › Copy with Overwrite All handles file-over-directory`
- `conflict-edge-cases.spec.ts › Type mismatch conflicts › Copy with Overwrite All handles directory-over-file`
- `conflict-move.spec.ts › Move multi-item merge (Layout B) › Move multi-item with Overwrite All merges and removes source`
- `conflict-move.spec.ts › Move multi-item merge (Layout B) › Move multi-item with Skip preserves source of skipped files`
- `conflict-move.spec.ts › Move rollback › Move rollback button is available and cancels operation`
- `file-operations.spec.ts › Copy round-trip › copies file-a.txt from left pane to right pane via F5`
- `file-operations.spec.ts › Move round-trip › moves file-b.txt from left pane to right pane via F6`

All failed at `waitForDialogsToClose` (modal-overlay never closes), meaning the F5/F6 dispatch landed but the dialog
flow stalled, or the dialog never opened in the first place. Pass 1 was a cold-build run on a hot machine; pass 2 (warm
cache, lighter machine state) shaved it down to 1 single failure in the same family. Variance is consistent with Step
6a's note that "different tests fail on each run, confirming this is parallel-load-induced variance, not a deterministic
regression."

Pass 2 (1 failure, non-mtp-1): `accessibility.spec.ts › light mode › Search dialog` (`.search-overlay` selector timeout
(same Cmd+F deferred flake).

None of these touch MTP code or the Step 6b fix. Documented here for completeness and to confirm the Step 6a backlog
remains the next thing to chase.

### Files touched

- `apps/desktop/src-tauri/src/commands/mtp.rs`: new `resync_virtual_mtp_after_disk_change` IPC command
- `apps/desktop/src-tauri/src/ipc.rs`: registered the new command (specta types + invoke handler)
- `apps/desktop/test/e2e-playwright/mtp.spec.ts`: `beforeEach` uses the combined IPC
- `apps/desktop/test/e2e-playwright/mtp-conflicts.spec.ts`: same
- `apps/desktop/test/e2e-playwright/playwright.config.ts`: dropped `retries: 1` on the MTP shard

### Wall-clock delta

The MTP shard added ~750 ms of settle time per test (600 + 150 ms across the two sleeps), spread over the 26 MTP tests =
~20 s of new fixed cost. The retry was worth ~5-10 s when it fired, ~0 when it didn't, so the net change is roughly even
on a green run. The big win is eliminating the 1-in-3 retry cost _and_ the false-confidence the retry masked. Pass 2
came in at **3m 20s** total (warm-cache, matches Step 6's 2m 47s baseline within build-noise).

## After Step 6d (Cancel-copy rollback bug fix)

- Date: 2026-05-12
- Machine: macOS, native, three Tauri instances
- Branch: `e2e-speedup` (worktree)

### Hypothesis

The Step 1 baseline flagged `Cancel copy mid-operation rolls back partial files` (`conflict-edge-cases.spec.ts:32`) as
both the slowest test (32.7 s) and a long-standing flake. The test copies the 23-file / 170 MB `bulk/` fixture, polls
for the Rollback button up to 10 s, clicks it, then expects either zero files remaining (rollback worked) or all 23
(escape hatch for "fast filesystem completed before rollback could land"). The 3-to-22-files window means the test
asserts `expect(remaining.length).toBeLessThan(3)` and fails. That's the flake.

Working theory before reading the code: the Rust copy loop has a race where the user's Rollback intent arrives between
the last per-file `is_cancelled` check and the loop's exit, so the loop returns `Ok(())`, the match arm goes to the
success path, `transaction.commit()` happens, files stay. The frontend dialog also keeps the Rollback button alive for
`MIN_DISPLAY_MS = 400 ms` after `write-complete` arrives, so a click during that window hits a backend whose state has
already been removed from `WRITE_OPERATION_STATE`, making it a silent no-op.

### What I found

Two real bugs, both real, both reachable on APFS (`/tmp` is APFS, `copyfile(3)` with `COPYFILE_CLONE` finishes 170 MB
across 23 files in < 100 ms):

1. **Lost-rollback on `Ok(())` (Rust, `copy.rs::copy_files_with_progress`):** the success arm did not check
   `state.intent`. When the user clicked Rollback _during_ the loop but the loop happened to finish before the next
   `is_cancelled()` poll (clonefile is essentially atomic per file: < 1 µs between record and next check), the result
   was `Ok(())`, we emitted `write-complete`, and the user's "delete what was copied" request evaporated.
2. **Click-after-settle window (Svelte, `TransferProgressDialog.svelte`):** the dialog stays open for the remainder of
   `MIN_DISPLAY_MS` after `write-complete` arrives so the final "100%" frame doesn't flash. The Rollback button was
   still enabled during that hold-open. By the time the user clicked, the backend had already removed the operation from
   `WRITE_OPERATION_STATE`, so `cancel_write_operation` was a no-op. UI showed "Rolling back…" briefly (because
   `isRollingBack = true` was set optimistically), then the dialog closed via the original `write-complete` timer.
   Nothing rolled back.

Both bugs are in the same family ("user wants the rollback, system says nothing happened"), and either one alone is
enough to make the test land in the 3-to-22-files window when only some files have been committed but the loop already
exited. The variability between runs comes from FS timing: how many files made it into `transaction.created_files`
before the loop's final lap.

### Fix

1. **Rust:** in `copy.rs::copy_files_with_progress`, the `Ok(())` arm now loads `OperationIntent` first. If it's
   `RollingBack`, we call `rollback_with_progress` (same path the `Err(Cancelled)` arm takes), commit the transaction,
   and emit `write-cancelled` instead of `write-complete`. The user's intent wins even when it arrived late.
2. **Svelte:** `operationSettled` is now `$state(false)` (it was a plain `let`), and the Cancel/Rollback buttons
   `disabled={isCancelling || operationSettled}`. Once the terminal event has arrived (complete / error / cancelled),
   clicking either is a no-op anyway, so it's better to disable them so the user gets honest feedback that the operation
   is over.

The `Stopped`-during-rollback semantics are unchanged: cancelling an in-progress rollback still keeps whatever hasn't
been deleted yet.

### Files touched

- `apps/desktop/src-tauri/src/file_system/write_operations/copy.rs`: `Ok(())` arm honors late `RollingBack` intent.
- `apps/desktop/src/lib/file-operations/transfer/TransferProgressDialog.svelte`: reactive `operationSettled` + button
  disable.
- `scripts/check/checks/file-length-allowlist.json`: `copy.rs` 825 → 857 (32 lines of rollback handling + a code comment
  explaining the late-intent case).

### Validation

Two back-to-back full-suite runs (`./scripts/check.sh --check desktop-e2e-playwright`, parallel shards, native macOS):

| Run    | Cancel-copy test duration | Result | Total runtime |
| ------ | ------------------------- | ------ | ------------- |
| Pass 1 | 742 ms (clean rollback)   | ✓      | 3m 26s        |
| Pass 2 | 31.2 s (escape-hatch)     | ✓      | 3m 12s        |

- **Cancel-copy: passes in both passes.** Pass 1 was the happy path: the Rollback click landed mid-loop, the new
  `Ok(())` check fired, rollback ran cleanly in well under a second, dialog closed immediately, test finished in 742 ms.
  That's a 44× speedup over the Step 1 baseline (32.7 s) for this specific test in the green case.
- **Pass 2 hit the escape-hatch path** (all 23 files remain, log "Rollback clicked but copy already completed"), used
  the full 30 s modal-close wait, but still passed. With the button-disable fix, this case is now honest: the click
  registered visually as disabled, the dialog stayed open for the `MIN_DISPLAY_MS` hold, then closed via the original
  `write-complete` timer. Files were never expected to roll back here; copy genuinely finished first.
- **Non-cancel flakes during validation:** all Step 6a-deferred keystroke-dispatch family (out of scope per Step 6d
  prompt). Pass 1 saw 8 of them (4 in `accessibility.spec.ts` dark-mode dialogs, 2 in `app.spec.ts` Transfer dialogs
  F5/F6, 1 in `indexing.spec.ts`, 2 in MTP (`mtp.spec.ts:657` cross-storage move and `mtp.spec.ts:906` external add via
  the `activeElement.closest('.dual-pane-explorer')` focus check)). Pass 2 was fully clean, confirming these are
  parallel-load variance, not a regression. Tracked in Step 6a backlog.

`./scripts/check.sh` full sweep: **all 45 checks green** in 2m 33s.

### Wall-clock impact

The fix shifts the Cancel-copy test from a flaky 32.7 s outlier to either ~750 ms (rollback ran) or ~31 s (escape
hatch). The escape-hatch duration is bounded by the test's own 30 s modal-close wait, not by anything we control here.
Tightening that timeout would be a Step 7 candidate; the rollback path itself completes in milliseconds.

## After Step 6e (F-key tests via dispatchMenuCommand)

### Goal

Eliminate the residual "synthesized KeyboardEvent doesn't always reach its handler under parallel-shard load" flake
observed in Steps 6a/6b/6d (0-1-2-5-10 flakes across runs, all clustered on `wait_for_selector` timeouts after an F-key
dispatch).

### Approach

Replace `tauriPage.keyboard.press('F5')`-style synthesized keystrokes with
`dispatchMenuCommand(tauriPage, 'file.copy')`, the helper in `helpers.ts` that emits the `execute-command` Tauri event
directly, mimicking what the OS native menu accelerator does in prod. The Tauri-event path is unaffected by DOM focus
state and parallel-load timing.

Rule of thumb: convert when the test cares about the resulting dialog / file state, keep keyboard when the test's title
or comments mark it as exercising the keyboard pathway itself (e.g. `app.spec.ts` "opens copy dialog with F5",
`file-operations.spec.ts` "...via F5", MTP read-only enforcement tests, MTP "renames file...via keyboard").

### What changed

**Converted to `dispatchMenuCommand` (28 dispatches across 16 tests):**

- `conflict-copy.spec.ts`: 7 × F5 → `file.copy`
- `conflict-move.spec.ts`: 3 × F6 → `file.move`
- `conflict-edge-cases.spec.ts`: 8 × F5 → `file.copy`
- `mtp-conflicts.spec.ts`: 5 × F6 → `file.move`
- `accessibility.spec.ts`: F5 → `file.copy`, F6 → `file.move`, F8 → `file.delete`, plus the ⌘F dispatch in
  `openSearchDialog()` → `search.open`
- `file-watching.spec.ts`: 1 × F5 → `file.copy` (the "in-app copy without duplicates" test)

**Kept on keyboard pathway (15 dispatches across 15 tests):**

- `app.spec.ts` (6): "opens new folder dialog with F7" ×2, "opens copy dialog with F5", "opens move dialog with F6",
  "Cancel button closes the new folder dialog" (uses F7), "opens the delete confirmation dialog with F8"
- `file-operations.spec.ts` (4): "...via F5", "...via F6", "...via F2", "...via F7" (round-trip tests with explicit
  F-key intent in their titles)
- `mtp.spec.ts` (5): F8 in "deletes file on MTP with 'Delete permanently' dialog" (comment marks it as full-keyboard
  flow), F2 ×2 in "renames file on MTP via keyboard" and "rename to existing name is rejected on MTP", F7 and F2 in
  "read-only storage rejects write operations" (test verifies the read-only pre-check fires from the keyboard path)

### Validation

Three back-to-back `./scripts/check.sh --check desktop-e2e-playwright` runs on native macOS with parallel shards:

| Run    | Result | Total | Per shard            | Flakes |
| ------ | ------ | ----- | -------------------- | ------ |
| Pass 1 | ✓      | 3m13s | 131 passed, 0 failed | 0      |
| Pass 2 | ✓      | 3m11s | 131 passed, 0 failed | 0      |
| Pass 3 | ✓      | 3m10s | 131 passed, 0 failed | 0      |

**The keystroke-dispatch flake is gone.** All three runs match the 0/0/0 target. No new flake categories surfaced.
`./scripts/check.sh` (full sweep) is green.

### Files touched

- `apps/desktop/test/e2e-playwright/conflict-copy.spec.ts`
- `apps/desktop/test/e2e-playwright/conflict-move.spec.ts`
- `apps/desktop/test/e2e-playwright/conflict-edge-cases.spec.ts`
- `apps/desktop/test/e2e-playwright/mtp-conflicts.spec.ts`
- `apps/desktop/test/e2e-playwright/accessibility.spec.ts`
- `apps/desktop/test/e2e-playwright/file-watching.spec.ts`

No app source changes (test files only, per Step 6e constraints).
