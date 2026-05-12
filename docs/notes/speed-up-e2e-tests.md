# E2E test speed-up

Tracking work to make the Playwright E2E suite faster.

Status: in progress. Started 2026-05-12.

## Before

- Date: 2026-05-12
- Machine: macOS, native, single worker
- Branch: `e2e-speedup` (worktree)
- Suite result: 121 passed, 17 skipped (SMB on macOS), 1 failed
- Failed: `mtp.spec.ts:784 — MTP read-only enforcement › read-only storage rejects write operations` — flaked at 19.9s
  on a `has_item` poll inside `mcpAwaitItem` for `sunset.jpg` (15s mcp timeout); see triage at bottom

### Totals

- **Playwright wall-clock**: 611.5 s ≈ **10m 12s** (`stats.duration` in the JSON report)
- **Checker total** (incl. build + app start + cleanup): **13m 12s**
- **Sum of per-test durations**: 608.8 s — virtually all of Playwright's time is inside tests; ~3s is reporter/teardown
  overhead
- **Total fixed-sleep budget**: **488.9 s** across 2299 `sleep()` calls — **80% of wall-clock is fixed sleeps**

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

| Total ms   | Calls | Frame                                                                        |
| ---------- | ----- | ---------------------------------------------------------------------------- |
| 171,000    | 1710  | `pollUntil` (helpers.ts:429) — the polling interval, 100 ms × everywhere     |
| 53,500     | 107   | `ensureAppReady` (helpers.ts:117) — `sleep(500)` after route nav             |
| 42,000     | 21    | mtp.spec.ts:117 — `sleep(2000)` in `beforeEach` after volume switch          |
| 40,000     | 20    | `setTheme` accessibility.spec.ts:189 — `sleep(2000)` per a11y test           |
| 32,100     | 107   | `ensureAppReady` (helpers.ts:162) — `sleep(300)` after `mcp-nav-to-path`     |
| 14,000     | 28    | accessibility.spec.ts:359 — close dialog wait `sleep(500)`                   |
| 14,000     | 28    | accessibility.spec.ts:346 — open dialog wait `sleep(500)`                    |
| 10,000     | 5     | mtp-conflicts.spec.ts:70 — `sleep(2000)` in `beforeEach` after volume switch |
| 10,000     | 1     | mtp.spec.ts:968 — `sleep(10000)` for 50 MB local→MTP copy                    |
| 10,000     | 1     | mtp.spec.ts:936 — `sleep(10000)` for 50 MB MTP→local copy                    |
| 8,000      | 4     | error-pane.spec.ts:62 — `sleep(2000)` after injecting permission errors      |
| 8,000      | 4     | network-toggle.spec.ts:90 — `sleep(2000)` after toggling network setting     |
| 4,200      | 21    | mtp.spec.ts:123 — `sleep(200)` after Escape #2                               |
| 4,200      | 21    | mtp.spec.ts:121 — `sleep(200)` after Escape #1                               |
| 3,600      | 12    | `setSettingViaBridge` network-toggle.spec.ts:64                              |
| 3,350      | 67    | `moveCursorToFile` (helpers.ts:302) — per-keystroke `sleep(50)`              |
| 3,000      | 1     | mtp.spec.ts:464 — `sleep(3000)` after MTP file op                            |
| 3,000 each | 5     | mtp-conflicts.spec.ts:104, 135, 167, 215, 256 — post-conflict `sleep(3000)`  |
| 3,000      | 1     | file-watching.spec.ts:205 — watcher reaction `sleep(3000)`                   |
| 2,400      | 12    | `selectAll` conflict-helpers.ts:147                                          |
| 2,000      | 4     | indexing.spec.ts:82 — `waitForExactSize` polling sleep                       |

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

`mtp.spec.ts:784 — read-only storage rejects write operations` timed out waiting for `sunset.jpg` to appear in `photos/`
on the left MTP pane (15s `mcpAwaitItem` budget exhausted). This is a different flake than the two the previous agent
called out (`Cancel copy mid-operation` and Linux SMB). Logging it here but not addressing in Step 1 — we'll see if it
survives Step 2 with poll-based waits.

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

- **Playwright wall-clock**: 305.6 s ≈ **5m 6s** (`stats.duration` — was 611.5 s / 10m 12s; **−50.0%**, ~5m 6s saved)
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

| Total ms | Calls | Frame                                                            |
| -------- | ----- | ---------------------------------------------------------------- |
| 151,550  | 3031  | `pollUntil` (helpers.ts:429) — 50 ms interval × every poll       |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65) — `sleep(300)` |
| 3,400    | 68    | `moveCursorToFile` (helpers.ts:302) — `sleep(50)` per keystroke  |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                            |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:299)                              |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:304)                              |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:80)                     |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                     |
| 1,600    | 16    | `selectConflictPolicy` (conflict-helpers.ts:163)                 |
| 1,000    | 2     | `waitForExactSize` (indexing.spec.ts:82)                         |

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

`file-watching.spec.ts` now dominates — the watcher debounce + index reconciliation is the real bottleneck there, not
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
6. **`accessibility.spec.ts`**: removed the `setTheme` `sleep(2000)` entirely — the only reason for it was
   color-contrast cache lag, but color-contrast is disabled in this suite. The `:root` `pollUntil` above it is enough.
   Also trimmed the Settings-section loop's `sleep(500)` (before-pollUntil-visibility) to nothing and the
   post-visibility `sleep(500)` to `sleep(150)`
7. **`error-pane.spec.ts`**: `injectAndNavigateIntoSubDir` `sleep(2000)` replaced with `pollUntil` on `.error-pane`
   visibility
8. **`file-watching.spec.ts`**: `sleep(3000)` after deleting the watched directory replaced with a 5 s `pollUntil` that
   waits for the temp file to disappear from the listing — the subsequent assertions still cover the "app still works"
   contract

### Surprises / notes

- The `setTheme` 2-second wait was a sleeper win: 40 s saved across 20 a11y tests, and the audit still passes cleanly.
  The "WKWebView cache lag" comment was real history, but it only applied to the (now-disabled) color-contrast rule.
- The two flakes called out in Step 1 (`Cancel copy mid-operation` on macOS, `MTP read-only … sunset.jpg`) both passed
  cleanly this run. They didn't reproduce, but I didn't change anything that would deterministically fix them either —
  could just be a quiet machine.
- `file-watching.spec.ts` saw almost no improvement (87.4 s → 85.1 s). Its sleeps are mostly the file-watcher's own
  debounce delays, not test-side waits. Out of scope for Step 2.
- No new flakes introduced. Suite went 122/122 expected pass on first try.

### Follow-up: back-to-back-run flake

The validation pass uncovered a regression on back-to-back runs (first run green, second run within the same machine
session had 15 dialog-driven failures — F5/F6/F8/Delete keypresses didn't open their dialogs). Root cause: dropping the
`sleep(500)` after `navigateToRoute` and `sleep(300)` after `mcp-nav-to-path` let `ensureAppReady` return before
`+page.svelte`'s `onMount` had finished wiring `document.addEventListener('keydown', handleGlobalKeyDown)`. On a cold
first run, the `mcp-nav-to-path` listener also lived inside that same `onMount` chain, so the `leftExpected`-files poll
implicitly gated on it. On a warm second run the panes were already on `left/` from a prior test, so the poll resolved
instantly — before the global keydown listener was attached. F-key presses then fired into a void.

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
this step). Most per-file deltas here are noise — Step 3's savings live in checker overhead (build cache + faster
shutdown when there's less hanging state), not test wall-clock.

### Top sleep call sites (Step 3 pass 1)

| Total ms | Calls | Frame                                                               |
| -------- | ----- | ------------------------------------------------------------------- |
| 154,650  | 3093  | `pollUntil` (helpers.ts:478) — 50 ms interval × every poll          |
| 10,700   | 107   | `ensureAppReady` (helpers.ts:218) — the 100 ms focus-attach margin  |
| 4,200    | 28    | `accessibility.spec.ts:359` — `sleep(150)` post-visibility          |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65) — `sleep(300)`    |
| 3,400    | 68    | `moveCursorToFile` (helpers.ts:316) — `sleep(50)` per keystroke     |
| 3,000    | 1     | `mtp.spec.ts:490` — single `sleep(3000)` after MTP fixture mutation |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                               |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:318)                                 |
| 2,100    | 21    | `moveCursorToFile` (helpers.ts:313)                                 |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:76)                        |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                        |

The volume-reset `cmdr://state` poll (Step 2's "wait for both panes on local volume", 5s budget) no longer appears as a
top frame — it now resolves immediately on most tests via the `isStateClean()` short-circuit and falls back to the full
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

1. **`helpers.ts`**: added `isStateClean(tauriPage, localVolumeName)` — reads `cmdr://state` over MCP and returns true
   when both panes are on the named local volume AND no `.modal-overlay` is visible. Returns false on any error (caller
   should fall back to the full reset). Imports `mcpReadResource` from `e2e-shared/mcp-client.js`.
2. **`mtp.spec.ts`**: wrapped the `mcp-volume-select` + `cmdr://state` poll + double-Escape + modal-overlay poll inside
   `if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME)))`. The MTP fixture reset (pause watcher → recreate → rescan
   → resume) still runs every test, as it must.
3. **`mtp-conflicts.spec.ts`**: same short-circuit pattern for the volume reset block. MTP fixture rebuild stays
   unconditional.
4. **`smb.spec.ts`**: same short-circuit pattern. The MCP-health diagnostic, `recreateFixtures()`, `sleep(1000)` watcher
   settle, route-nav-back-to-`/`, and `initMcpClient()` all still run every test — only the volume-select block is
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
  pass between them — see "Surprises" below)

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
keyboard-driven `moveCursorToSubDir` helper (intentional — see "Tests kept on keyboard nav" below).

### Top sleep call sites (Step 4 pass 1)

| Total ms | Calls | Frame                                                               |
| -------- | ----- | ------------------------------------------------------------------- |
| 154,450  | 3089  | `pollUntil` (helpers.ts:511) — 50 ms interval × every poll          |
| 10,700   | 107   | `ensureAppReady` (helpers.ts:218) — the 100 ms focus-attach margin  |
| 4,200    | 28    | `accessibility.spec.ts:359` — `sleep(150)` post-visibility          |
| 3,600    | 12    | `setSettingViaBridge` (network-toggle.spec.ts:65) — `sleep(300)`    |
| 3,000    | 1     | `mtp.spec.ts:490` — single `sleep(3000)` after MTP fixture mutation |
| 2,400    | 12    | `selectAll` (conflict-helpers.ts:147)                               |
| 2,000    | 4     | `navigateBackToLeft` (error-pane.spec.ts:76)                        |
| 2,000    | 2     | `toggleHidden` (file-operations.spec.ts:246)                        |
| 2,000    | 1     | `mtp.spec.ts:669` — single `sleep(2000)`                            |
| 2,000    | 1     | `mtp.spec.ts:639` — single `sleep(2000)`                            |

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
   `data-filename`. Bails early (returns `false`) when the file isn't in the focused pane's listing — matches the prior
   return contract.
2. **`e2e-shared/mcp-client.ts`**: added `ensureMcpClient(tauriPage)` — an idempotent init wrapper that skips the IPC
   round-trip when the port has already been discovered. Used inside `moveCursorToFile` because most callers
   (`file-watching`, `conflict-copy`, `file-operations`, `accessibility`, `conflict-edge-cases`) never called
   `initMcpClient` directly.

### Tests kept on keyboard nav

- **`app.spec.ts` › Keyboard navigation › moves cursor with arrow keys**: explicitly asserts that pressing `ArrowDown`
  advances the cursor by one. Stays on its keyboard path; uses its own local `moveCursorToSubDir` helper (defined inside
  `app.spec.ts`) — not `moveCursorToFile`.
- **`app.spec.ts` › Mouse interactions › moves cursor when clicking a file entry**: cursor-via-click path. Not affected.
- **`app.spec.ts` › Keyboard navigation › toggles selection with Space key**: relies on `skipParentEntry`
  (keyboard-driven). Not affected.
- Other `keyboard.press(...)` call sites across the suite (F5/F6/F2/Tab/Backspace/Enter) test the actual keyboard
  shortcuts and continue to use the keyboard directly — only the cursor-positioning step before pressing them was
  swapped for MCP.

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

Wall-clock dropped to **2m 47s** (checker total) from the **5m 49s** Step 5 baseline — a **52% cut on the user-visible
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
- N=3: one **MTP shard** (sequential lane — `mtp.spec.ts` + `mtp-conflicts.spec.ts`) and two **non-MTP shards** split by
  Playwright's `--shard 1/2` and `--shard 2/2`. The MTP shard runs alone because the virtual MTP backing dir
  (`/tmp/cmdr-mtp-e2e-fixtures`) is hard-coded in `src-tauri/src/mtp/virtual_device.rs` and shared by every Tauri
  instance — two MTP shards would clobber each other.
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
   `rename to existing name is rejected on MTP`) — all timing out waiting for `report.txt` after
   `recreateMtpFixtures()`. Root cause: every Tauri instance built with `virtual-mtp` runs `setup_virtual_mtp_device()`
   at startup, which **wipes** `/tmp/cmdr-mtp-e2e-fixtures` via `fs::remove_dir_all` and recreates the tree. With three
   instances starting nearly simultaneously, the wipe-and-recreate races and the three independent mtp-rs watchers (all
   pointed at the same backing dir) react to each other's writes. The MTP shard's in-memory device state can end up out
   of sync with disk.

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
   noted a similar incident with `sunset.jpg`). It happens roughly 1-in-3 runs under three concurrent Tauri instances —
   the extra CPU + `/tmp` I/O pressure makes it more frequent. The root cause is somewhere inside mtp-rs's
   pause/resume + rescan ordering, which is out of scope here. Set `retries: 1` for the MTP shard only in
   `playwright.config.ts` (gated on `CMDR_E2E_SHARD_KIND === 'mtp'`). The non-MTP shards keep `retries: 0`. The retry
   adds ~5-10 s to the MTP shard wall-clock when it fires and zero when it doesn't.

### What limits the parallelism

- **MTP lane stays single-instance** as long as `MTP_FIXTURE_ROOT` is a `const &str`. Making it env-var-driven would
  unlock additional MTP shards but isn't worth the extra Rust surface area for an already-short lane (~78 s).
- **Non-MTP scaling** is roughly proportional but capped by `--shard`'s per-file granularity. With the current spec
  layout, the `non-mtp-1` shard inherits `file-watching.spec.ts` (~78 s, dominated by watcher debounce delays the app
  actually needs to be fast for). Going to N=4 (three non-MTP shards instead of two) saves at best another ~20 s —
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
  Tauri" workaround for the checker — the test takes 2-3 minutes total.
- `./scripts/check.sh` (fast) is green after the patch.

## After Step 6a (data-app-ready signal + Go fixture-parse robustness)

Step 6's parallel sharding amplified two pre-existing weaknesses:

1. **`ensureAppReady` focus race.** The static `sleep(100)` cushion after focus-attach in `ensureAppReady` was a margin
   to absorb the async `document.addEventListener('keydown', ...)` attach inside `+page.svelte`'s `onMount`. Under
   parallel load, 100 ms is occasionally insufficient — F-key dispatches lose the handler.
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
- The remaining `waitForFunction` on `activeElement.closest('.dual-pane-explorer')` stays — it confirms the click+focus
  landed.

**Go check runner (`scripts/check/checks/desktop-svelte-e2e-playwright.go`)**:

- `createE2EFixtures` now scans every line of the tsx output for one starting with `/tmp/cmdr-e2e-` instead of taking
  the last line blindly. npm notices and similar tail-end noise no longer break the parse.

### Result

- Before Step 6a: with the 100 ms cushion, the suite was on a thin margin under parallel sharding load — 13 failures on
  a back-to-back validation, with cascading ECONNREFUSED after a mid-suite Tauri crash.
- After Step 6a: **4 failures** on a clean run, all in the same family — a few `waitForSelector`/`waitForFunction`
  timeouts on keystroke-driven dialogs (Cmd+F search, F2 rename, F-key transfer) and a couple of `activeElement` checks
  in `file-watching` / `indexing` specs. Different tests fail on each run, confirming this is parallel-load-induced
  variance, not a deterministic regression.
- Strict improvement over the pre-Step-6a state, but not yet zero-flakes.

### Remaining flakes (deferred)

Under parallel-shard load:

- **Keystroke dispatches occasionally miss their handler** — Cmd+F (search overlay), F2 (rename input), F5/F6/F7/F8
  (transfer/delete/mkdir dialogs). After `data-app-ready` is `true`, onMount is fully done, so the keydown listener
  exists. Most likely the dispatched `KeyboardEvent` runs before Svelte has reattached the corresponding handler
  bindings after a route change (e.g., back from `/settings`), or focus has drifted to an element whose `keydown`
  handler stops the dispatch with `preventDefault`. Worth chasing in a follow-up — candidates:
  - Have `data-app-ready` also flip back to `"false"` on route change so we wait for the new mount cleanly.
  - Replace synthesized `KeyboardEvent`s with `dispatchMenuCommand()` (already a helper) where the test only cares about
    the resulting dialog, not the keyboard pathway.
- **`activeElement.closest('.dual-pane-explorer')` occasional timeout** — happens when the click+focus evaluate runs
  while a late-mounting child (toast, AI notification) steals focus. Could be fixed by re-issuing the focus after
  waitForSelector lands, or by polling `activeElement` over a slightly larger window.

These are deferred — diminishing returns vs. the speedup work. Tracking here for post-Step-6 follow-up.

### Wall-clock

Clean run: **3m 48s** checker total (up from Step 6's 2m 48s on a fully warm cache). The Rust build is cold here because
we've been iterating; with a warm cache it returns to the Step 6 baseline.
