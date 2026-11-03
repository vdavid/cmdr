/**
 * The choreography of each marketing clip: real UI actions on the staged demo tree
 * (`marketing-clips-demo.ts`), recorded by `marketing-clips-take.ts`.
 *
 * Each scene stages its window, starts a take, marks start / poster / end around the
 * actions a viewer should see, and puts the demo tree back afterwards so the next take
 * (the other theme) starts from the same state.
 *
 * ❗ `hold()` is CHOREOGRAPHY, not a readiness wait: it's how long a viewer gets to read a
 * frame before the next action. Every wait for the app is a real probe (`waitForSelector`,
 * `expect.poll`, `clickButtonByText`'s actionability poll), per `test/CLAUDE.md`.
 */

import { waitBudget } from './wait-budget.js'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { mcpCall } from '../e2e-shared/mcp-client.js'
import { expect } from './fixtures.js'
import {
  TRANSFER_DIALOG,
  clickButtonByText,
  closeScopedWindow,
  dismissAllToasts,
  dispatchMenuCommand,
  openQueueWindow,
  openSettingsWindowViaProd,
  openViewerWindow,
  pointerClick,
  pressKey,
  selectContentExpr,
  sleep,
} from './helpers.js'
import { clickTransferStart, confirmRollback } from './conflict-helpers.js'
import {
  SEARCH_INPUT,
  SEARCH_OVERLAY,
  closeSearchDialog,
  getActiveMode,
  openSearchDialog,
  resetSearchDialog,
} from './search-helpers.js'
import { appPid, setWindowSize, windowMetrics } from './marketing-shots-helpers.js'
import { focusApp, requireWindowId } from './marketing-shots-native.js'
import {
  pinVolatileChrome,
  resetTabs,
  setSetting,
  setTheme,
  setViewMode,
  stageCosmetics,
  waitForIndexedSizes,
} from './marketing-stage.js'
import {
  AI_QUERY,
  ARTWORK,
  DEMO,
  DEMO_ROOT,
  QUEUE_SOURCES,
  SEARCH_QUERY,
  VIEWER_DOC,
  resetBackup,
  resetRawImports,
} from './marketing-clips-demo.js'
import { captionKey, startTake } from './marketing-clips-take.js'
import { pressCaptioned, runCaptioned, typeCaptioned } from './marketing-clips-keys.js'
import type { ClipTheme, Take } from './marketing-clips-take.js'

const LOG = '[marketing-clips]'

/**
 * The main window's logical size in every main-window clip. Smaller than the stills'
 * 1142x705 on purpose: a clip ships at ~1,500 px wide, and a 2,000 px master scales by
 * 0.75 rather than 0.66, which is what keeps file-list text readable on a phone.
 */
const CLIP_MAIN = { width: 1000, height: 640 }

/** The viewer window's size. Differs from `CLIP_MAIN`, which window matching relies on. */
const CLIP_VIEWER = { width: 960, height: 680 }

/** The queue window's size, small enough to sit inside the main window's frame. */
const CLIP_QUEUE = { width: 720, height: 470 }

/** The settings window's size in the search clip: inside the main window's frame, like the queue's. */
const CLIP_SETTINGS = { width: 800, height: 540 }

/**
 * The still beat after a clip's start marker and before its first action, so a viewer
 * sees where the clip begins before anything moves (and the loop's seam reads as a pause).
 */
const LEAD_IN_MS = 800

const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'
const MULTI_RENAME = '[data-dialog-id="multi-rename"]'

/** Lets a frame sit on screen. Choreography only; see the file header. */
export function hold(ms: number): Promise<void> {
  return sleep(ms)
}

/**
 * Types `text` into `selector` one character at a time, the way a person would, through
 * `input` events so the bound state follows. `perCharMs` is choreography.
 */
export async function typeSlowly(page: TauriPage, selector: string, text: string, perCharMs = 85): Promise<void> {
  const sel = JSON.stringify(selector)
  const found = await page.evaluate<boolean>(`(function(){
      var el = document.querySelector(${sel});
      if (!el) return false;
      el.focus();
      var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
      desc.set.call(el, '');
      el.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    })()`)
  if (!found) throw new Error(`typeSlowly: ${selector} isn't on screen`)
  for (let i = 1; i <= text.length; i++) {
    await page.evaluate(`(function(){
        var el = document.querySelector(${sel});
        var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
        desc.set.call(el, ${JSON.stringify(text.slice(0, i))});
        el.dispatchEvent(new Event('input', { bubbles: true }));
      })()`)
    await hold(perCharMs)
  }
}

/** Opens the search dialog with its shortcut, captioned. */
async function openSearchCaptioned(page: TauriPage): Promise<void> {
  await runCaptioned(page, 'search.open')
  await page.waitForSelector(SEARCH_OVERLAY, waitBudget(3000))
}

/** Closes the search dialog with Esc, captioned once. */
async function closeSearchCaptioned(page: TauriPage): Promise<void> {
  captionKey('Esc')
  await closeSearchDialog(page)
}

export async function invoke(page: TauriPage, command: string, args: Record<string, unknown>): Promise<void> {
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)
}

/** Per-file copy pace and per-item rollback pace (`src-tauri/src/test_mode.rs`); `null` clears. */
export async function setThrottles(page: TauriPage, copyMs: number | null, rollbackMs: number | null): Promise<void> {
  await invoke(page, 'set_test_throttle', { ms: copyMs })
  await invoke(page, 'set_test_rollback_throttle', { ms: rollbackMs })
}

/** Cancels whatever is still running and waits for the operation lanes to empty. */
export async function drainOperations(page: TauriPage): Promise<void> {
  await page.evaluate(`(async function() {
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
  })()`)
  await expect
    .poll(
      async () => {
        await page.evaluate(`window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations').catch(function(){})`)
        return page.evaluate<number>(
          `window.__TAURI_INTERNALS__.invoke('list_operations').then(function(o){ return o.length })`,
        )
      },
      { timeout: waitBudget(30000) },
    )
    .toBe(0)
}

/**
 * Puts the main window in the state every main-window clip starts from, in `theme`.
 * Idempotent, like the stills' `stageMainWindow`: a previous take may have left a
 * dialog up or a pane somewhere else.
 */
export async function stageMain(
  page: TauriPage,
  theme: ClipTheme,
  left: string,
  right: string,
  look: { size: { width: number; height: number }; zoom: 100 | 125 } = { size: CLIP_MAIN, zoom: 100 },
): Promise<void> {
  // Zoom first: it changes nothing about the window's size, but set after the resize it
  // would reflow the panes the size poll below just waited for.
  await dispatchMenuCommand(page, `view.zoom.set${String(look.zoom)}`)
  await setWindowSize(page, 'main', look.size.width, look.size.height)
  await expect
    .poll(async () => (await windowMetrics(page, 'main')).logical.width, { timeout: waitBudget(5000) })
    .toBe(look.size.width)
  await waitForIndexedSizes(page, LOG)
  await stageCosmetics(page)
  // A beat that showed the hidden .git folder may have left hidden files on; the demo
  // tree's own marker file is hidden and must stay out of frame.
  await setSetting(page, 'listing.showHiddenFiles', false)
  // The `undo` video beat turns this on; left on, every later copy's dialog waits for a Close.
  await setSetting(page, 'fileOperations.keepProgressOpenWhenDone', false)
  await cancelSignInSheet(page)
  await resetTabs('left')
  await resetTabs('right')
  await setViewMode('left', 'full')
  await setViewMode('right', 'full')
  await mcpCall('nav_to_path', { pane: 'left', path: left })
  await mcpCall('nav_to_path', { pane: 'right', path: right })
  await cancelSignInSheet(page)
  const leftFocused = await page.evaluate<boolean>(
    `document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') ?? false`,
  )
  if (!leftFocused) await mcpCall('switch_pane', {})
  await waitForExactSizes(page)
  await pinVolatileChrome(page)
  await setTheme(page, theme)
  await dismissAllToasts(page)
}

const SIGN_IN_SHEET = '[data-dialog-id="server-sign-in"]'

/**
 * Cancels a server sign-in sheet if one is up. The app reopens its panes where they were,
 * and a pane left on the `--video2` run's NAS share asks to sign in again when the SMB
 * Docker fixture is down; the sheet then blocks every later step (2026-10-26).
 */
async function cancelSignInSheet(page: TauriPage): Promise<void> {
  if (!(await page.isVisible(SIGN_IN_SHEET))) return
  await clickButtonByText(page, `${SIGN_IN_SHEET} button`, 'Cancel')
  await expect.poll(async () => !(await page.isVisible(SIGN_IN_SHEET)), { timeout: waitBudget(5000) }).toBeTruthy()
}

/**
 * Waits until every folder in both panes shows an exact size: no `≥` (a lower bound
 * while the index hasn't aggregated a folder) and no `<dir>` (not indexed yet). The demo
 * tree is seconds old when the first take stages, so `waitForIndexedSizes` passes while
 * its folders still read either (seen on the first real run, 2026-10-22).
 */
async function waitForExactSizes(page: TauriPage): Promise<void> {
  // Soft: an inexact size is cosmetic, and a take is worth more than a size. A folder the
  // index never sizes (the `.git` portal's repo, a zip) would otherwise fail its beat at
  // the timeout (first video run, 2026-10-23), so this warns and goes on.
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(
          `[...document.querySelectorAll('.file-pane')].some((p) => p.textContent.includes('≥') || p.textContent.includes('<dir>'))`,
        ),
      { timeout: waitBudget(45000), intervals: [500] },
    )
    .toBe(false)
    .catch(() => {
      console.warn(`${LOG} a folder size still reads ≥ or <dir> after 45 s; recording anyway`)
    })
}

/**
 * Brings the app (and `label`'s window) to the front and starts a take of it.
 *
 * Front matters even though ScreenCaptureKit records a covered window: WKWebView starves
 * `requestAnimationFrame` in an occluded window, so a clip recorded behind the terminal
 * would show frozen progress bars.
 */
export async function recordWindow(
  page: TauriPage,
  options: { slug: string; theme: ClipTheme; label: string; include?: string[] },
): Promise<Take> {
  const pid = appPid()
  focusApp(pid)
  const metrics = await windowMetrics(page, options.label)
  expect(metrics.scale, 'the clips are recorded at 2x; run this on a retina display').toBe(2)
  const windowId = requireWindowId(pid, metrics.logical, options.label)
  const includeWindowIds: number[] = []
  for (const label of options.include ?? []) {
    const included = await windowMetrics(page, label)
    includeWindowIds.push(requireWindowId(pid, included.logical, label))
  }
  return startTake({ slug: options.slug, theme: options.theme, windowId, includeWindowIds })
}

/** Runs `body` inside a take, abandoning the take (never a half master) when it throws. */
export async function inTake(take: Take, body: () => Promise<void>): Promise<void> {
  try {
    await body()
  } catch (e) {
    await take.abandon()
    throw e
  }
  await take.finish()
}

// ── The scenes ──────────────────────────────────────────────────────────────

/** A throttled copy, Pause, Resume, then Rollback, and the bar walks back to zero. */
export async function copyRollback(page: TauriPage, theme: ClipTheme): Promise<void> {
  resetBackup()
  await stageMain(page, theme, join(DEMO_ROOT, DEMO.photos), join(DEMO_ROOT, DEMO.backup))
  await mcpCall('move_cursor', { pane: 'left', filename: 'Iceland 2026' })
  // 64 photos at 110 ms each is a ~7 s copy; Rollback lands around two thirds in, and
  // the reversal's 70 ms per file walks the bar back over ~3 s.
  await setThrottles(page, 110, 70)
  try {
    const take = await recordWindow(page, { slug: 'copy-rollback', theme, label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)
      await runCaptioned(page, 'file.copy')
      await page.waitForSelector(TRANSFER_DIALOG, waitBudget(5000))
      await hold(700)
      await clickTransferStart(page)
      await page.waitForSelector(PROGRESS_DIALOG, waitBudget(5000))
      await hold(2000)
      take.markPoster()
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Pause')
      await hold(1400)
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Resume')
      await hold(1500)
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Rollback')
      await page.waitForSelector('[data-dialog-id="rollback-confirmation"]', waitBudget(5000))
      await hold(900)
      await confirmRollback(page)
      await expect
        .poll(async () => !(await page.isVisible(PROGRESS_DIALOG)), { timeout: waitBudget(30000) })
        .toBeTruthy()
      await hold(700)
    })
  } finally {
    await setThrottles(page, null, null)
    await drainOperations(page)
    await dismissAllToasts(page)
    resetBackup()
  }
}

/** The queue window over the main window, five transfers one after the other. */
export async function opsQueue(page: TauriPage, theme: ClipTheme): Promise<void> {
  resetBackup()
  await stageMain(page, theme, join(DEMO_ROOT, DEMO.projects), join(DEMO_ROOT, DEMO.backup))
  await setThrottles(page, 90, null)
  const queue = await openQueueWindow(page)
  try {
    await placeQueueOverMain(page)
    const take = await recordWindow(page, { slug: 'ops-queue', theme, label: 'main', include: ['queue'] })
    // Above the main window, which `focusApp` may have raised.
    await invoke(page, 'plugin:window|set_focus', { label: 'queue' })
    await inTake(take, async () => {
      await hold(800)
      take.markStart()
      await hold(LEAD_IN_MS)
      for (const [i, source] of QUEUE_SOURCES.entries()) {
        await startCopy(page, join(DEMO_ROOT, DEMO.projects, source), join(DEMO_ROOT, DEMO.backup))
        if (i === 2) take.markPoster()
        await hold(350)
      }
      await expect
        .poll(
          () =>
            page.evaluate<number>(
              `window.__TAURI_INTERNALS__.invoke('list_operations').then(function(o){ return o.length })`,
            ),
          { timeout: waitBudget(60000) },
        )
        .toBe(0)
      await hold(700)
    })
  } finally {
    await setThrottles(page, null, null)
    await drainOperations(page)
    await closeScopedWindow(page, queue, 'queue')
    await dismissAllToasts(page)
    resetBackup()
  }
}

/**
 * Starts a local copy over IPC, exactly what the F5 dialog dispatches, with no modal.
 *
 * Fire-and-forget inside the webview: `copy_between_volumes` answers only once its
 * pre-flight has run, and awaiting it serialized the five starts a second apart, so the
 * queue never held more than one row (first real run, 2026-10-22).
 */
async function startCopy(page: TauriPage, source: string, destination: string): Promise<void> {
  const args = {
    sourceVolumeId: 'root',
    sourcePaths: [source],
    destVolumeId: 'root',
    destPath: destination,
    config: {
      conflictResolution: 'rename',
      progressIntervalMs: 100,
      maxConflictsToShow: 10,
      previewId: null,
      preKnownConflicts: [],
    },
  }
  await page.evaluate(`void window.__TAURI_INTERNALS__.invoke('copy_between_volumes', ${JSON.stringify(args)})`)
}

/**
 * Sizes the queue window and parks it over the main window's lower right, inside the
 * main window's frame: the take is cropped to that frame, so anything outside it would
 * be cut off.
 */
export async function placeQueueOverMain(page: TauriPage): Promise<void> {
  await setWindowSize(page, 'queue', CLIP_QUEUE.width, CLIP_QUEUE.height)
  await expect
    .poll(async () => (await windowMetrics(page, 'queue')).logical.width, { timeout: waitBudget(5000) })
    .toBe(CLIP_QUEUE.width)
  await page.evaluate(`(async function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    var pos = await invoke('plugin:window|outer_position', { label: 'main' });
    var main = await invoke('plugin:window|inner_size', { label: 'main' });
    var queue = await invoke('plugin:window|inner_size', { label: 'queue' });
    var scale = await invoke('plugin:window|scale_factor', { label: 'main' });
    var margin = Math.round(36 * scale);
    await invoke('plugin:window|set_position', {
      label: 'queue',
      value: { Physical: { x: pos.x + main.width - queue.width - margin, y: pos.y + main.height - queue.height - margin } },
    });
  })()`)
}

/** Patterns and a counter filling the live preview, then Rename. */
export async function multiRename(page: TauriPage, theme: ClipTheme): Promise<void> {
  resetRawImports()
  await stageMain(page, theme, join(DEMO_ROOT, DEMO.rawImports), join(DEMO_ROOT, DEMO.trip))
  await dispatchMenuCommand(page, 'selection.selectAll')
  try {
    await resetMultiRenameMask(page)
    const take = await recordWindow(page, { slug: 'multi-rename', theme, label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)
      await runCaptioned(page, 'file.multiRename')
      await page.waitForSelector(MULTI_RENAME, waitBudget(5000))
      await hold(900)
      await typeCaptioned('Iceland [Y-M-D] [C:3]', (text) =>
        typeSlowly(page, `${MULTI_RENAME} input[aria-label="Name mask"]`, text),
      )
      await hold(1300)
      take.markPoster()
      await hold(700)
      // The button reads "Rename 18 files"; match its start, the count is the engine's.
      await page.evaluate(`(function(){
          var buttons = document.querySelectorAll(${JSON.stringify(`${MULTI_RENAME} button`)});
          for (var i = 0; i < buttons.length; i++) {
            if (buttons[i].textContent.trim().indexOf('Rename ') === 0 && !buttons[i].disabled) { buttons[i].click(); return; }
          }
          throw new Error('no enabled Rename button');
        })()`)
      await expect.poll(async () => !(await page.isVisible(MULTI_RENAME)), { timeout: waitBudget(15000) }).toBeTruthy()
      await hold(1800)
    })
  } finally {
    if (await page.isVisible(MULTI_RENAME)) await clickButtonByText(page, `${MULTI_RENAME} button`, 'Cancel')
    await undoLastRename(page).catch((e: unknown) => {
      console.warn(`${LOG} couldn't undo the take's rename: ${String(e)}`)
    })
    await dispatchMenuCommand(page, 'selection.deselectAll')
    await dismissAllToasts(page)
    resetRawImports()
  }
}

/**
 * Off camera: undoes the take's rename through the sheet's own "Undo rename", which also
 * forgets the run. Without it the next take's sheet (the other theme) shows an "Undo
 * rename" button the first take's didn't have.
 */
async function undoLastRename(page: TauriPage): Promise<void> {
  await dispatchMenuCommand(page, 'selection.selectAll')
  await dispatchMenuCommand(page, 'file.multiRename')
  await page.waitForSelector(MULTI_RENAME, waitBudget(5000))
  await page.evaluate(`(function(){
    var b = [...document.querySelectorAll(${JSON.stringify(`${MULTI_RENAME} button`)})].find(function (x) { return (x.textContent || '').trim().indexOf('Undo rename') === 0 });
    if (b) b.click();
  })()`)
  await drainOperations(page)
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(
          `![...document.querySelectorAll(${JSON.stringify(`${MULTI_RENAME} button`)})].some(function (x) { return (x.textContent || '').trim().indexOf('Undo rename') === 0 })`,
        ),
      { timeout: waitBudget(10000) },
    )
    .toBe(true)
  if (await page.isVisible(MULTI_RENAME)) await clickButtonByText(page, `${MULTI_RENAME} button`, 'Cancel')
}

/**
 * Off camera: opens the sheet, puts the name mask back to `[N]`, and closes it. The sheet
 * opens on what the last one closed with, so the second take would otherwise open with
 * the finished mask already showing its preview, and the typing would change nothing.
 */
export async function resetMultiRenameMask(page: TauriPage): Promise<void> {
  const mask = `${MULTI_RENAME} input[aria-label="Name mask"]`
  await dispatchMenuCommand(page, 'file.multiRename')
  await page.waitForSelector(mask, waitBudget(5000))
  await typeSlowly(page, mask, '[N]', 0)
  // Wait for the preview of `[N]` (the footer counts "0 to rename") before closing, so the
  // sheet closes on it rather than on a pending edit.
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(
          `/^0 to rename/.test(document.querySelector(${JSON.stringify(`${MULTI_RENAME} .counts`)})?.textContent?.trim() ?? '')`,
        ),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  await clickButtonByText(page, `${MULTI_RENAME} button`, 'Cancel')
  await expect.poll(async () => !(await page.isVisible(MULTI_RENAME)), { timeout: waitBudget(5000) }).toBeTruthy()
}

/** The local model's runtime facts the AI scenes wait on (`get_ai_runtime_status`). */
interface LocalAiRuntime {
  serverRunning: boolean
  modelInstalled: boolean
}

async function localAiRuntime(page: TauriPage): Promise<LocalAiRuntime> {
  return page.evaluate<LocalAiRuntime>(`window.__TAURI_INTERNALS__.invoke('get_ai_runtime_status')`)
}

/** Waits until the local model's server answers `running`, on or off. */
async function waitForLocalAiServer(page: TauriPage, running: boolean): Promise<void> {
  await expect
    .poll(async () => (await localAiRuntime(page)).serverRunning, {
      timeout: waitBudget(90000),
      message: running ? "the local AI server didn't start" : "the local AI server didn't stop",
    })
    .toBe(running)
}

/**
 * Off camera: switches the instance to Cmdr's built-in local model and waits for its
 * server. On an instance without the model it downloads it first, through the same
 * `start_ai_download` that Settings > AI > Download calls (about 2 GB; 90 s on a fast
 * line, 2026-10-23). The model stays in the clips data dir for later runs, and an
 * interrupted download resumes. No API key anywhere.
 */
export async function ensureLocalAi(page: TauriPage): Promise<void> {
  await setSetting(page, 'ai.provider', 'local')
  if (!(await localAiRuntime(page)).modelInstalled) {
    console.log(`${LOG} downloading the local AI model into the clips instance…`)
    // Fire and forget: the command resolves only when the download ends, far past one
    // `evaluate`'s budget. The runtime status is the probe.
    await page.evaluate(`(function () { window.__TAURI_INTERNALS__.invoke('start_ai_download'); return true })()`)
    await expect
      .poll(async () => (await localAiRuntime(page)).modelInstalled, {
        timeout: waitBudget(480000),
        intervals: [5000],
        message: "the local AI model didn't finish downloading; run the take again to resume it",
      })
      .toBe(true)
  }
  await waitForLocalAiServer(page, true)
}

/** The file the AI query should find: last year's tax report (`stageTaxes`). */
export function lastYearsTaxReport(): string {
  return `Tax report ${String(new Date().getUTCFullYear() - 1)}`
}

/**
 * Waits for the AI search's answer to `AI_QUERY`: last year's tax report listed, and
 * nothing but tax files. The local model samples (`--temp 0.6`) and once answered with
 * the date range alone, so a wrong answer fails the take and Playwright's retry records
 * it again (2026-10-23).
 */
export async function waitForTaxAnswer(page: TauriPage): Promise<void> {
  const report = lastYearsTaxReport()
  await expect
    .poll(
      async () => {
        const rows = await page.evaluate<string[]>(
          `[...document.querySelectorAll('${SEARCH_OVERLAY} .result-row')].map(function (r) { return r.textContent || '' })`,
        )
        return rows.some((r) => r.includes(report)) && rows.every((r) => r.includes('Tax '))
      },
      { timeout: waitBudget(30000), message: `the AI search didn't list only tax files, ${report} among them` },
    )
    .toBe(true)
}

/**
 * Sizes `label`'s window to `size` and centers it over the main window, inside the main
 * window's frame: a take is cropped to that frame, so anything outside it would be cut off.
 */
async function centerOverMain(page: TauriPage, label: string, size: { width: number; height: number }): Promise<void> {
  await setWindowSize(page, label, size.width, size.height)
  await expect
    .poll(async () => (await windowMetrics(page, label)).logical.width, { timeout: waitBudget(5000) })
    .toBe(size.width)
  await page.evaluate(`(async function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    var pos = await invoke('plugin:window|outer_position', { label: 'main' });
    var main = await invoke('plugin:window|outer_size', { label: 'main' });
    var other = await invoke('plugin:window|outer_size', { label: ${JSON.stringify(label)} });
    await invoke('plugin:window|set_position', {
      label: ${JSON.stringify(label)},
      value: { Physical: { x: pos.x + Math.round((main.width - other.width) / 2), y: pos.y + Math.round((main.height - other.height) / 2) } },
    });
  })()`)
}

/**
 * A filename search with AI off, then AI turned on in Settings (the built-in local model),
 * then the same dialog asked a plain sentence, and only last year's tax files come back.
 *
 * The settings window waits BEHIND the main window, centered inside its frame, from before
 * the take: the recorder excludes every window that exists when it starts except the ones
 * it's given, so a window opened mid-take would show up wherever macOS put it. Raising it
 * and lowering it again is what the viewer sees as Settings opening and closing.
 */
export async function search(page: TauriPage, theme: ClipTheme): Promise<void> {
  await stageMain(page, theme, DEMO_ROOT, join(DEMO_ROOT, DEMO.documents))
  // Off camera: the model downloaded and proven to start, then AI off for the clip's opening.
  await ensureLocalAi(page)
  await setSetting(page, 'ai.provider', 'off')
  await waitForLocalAiServer(page, false)
  // The dialog keeps the last query and mode across opens: clear it, in Filename mode.
  await openSearchDialog(page)
  await resetSearchDialog(page)
  await expect.poll(() => getActiveMode(page), { timeout: waitBudget(5000) }).toBe('filename')
  await closeSearchDialog(page)
  // The reset can end on a "Couldn't finish that search" toast for the empty query, which
  // otherwise opens the clip (first real run, 2026-10-23).
  await dismissAllToasts(page)

  const settings = await openStagedSettings(page, theme)
  try {
    await expect
      .poll(
        () =>
          settings.evaluate<boolean>(`(function () {
            var item = [...document.querySelectorAll('.section-item')].find((s) => s.textContent.trim() === 'Provider');
            if (item) item.click();
            return !!document.getElementById('setting-ai.provider');
          })()`),
        { timeout: waitBudget(5000) },
      )
      .toBe(true)
    await centerOverMain(page, 'settings', CLIP_SETTINGS)
    const localOption = `[...document.getElementById('setting-ai.provider').querySelectorAll('button[role="radio"]')].find((b) => b.textContent.trim() === 'Local LLM')`

    const take = await recordWindow(page, { slug: 'search', theme, label: 'main', include: ['settings'] })
    // Main in front, settings tucked behind it.
    await invoke(page, 'plugin:window|set_focus', { label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)

      // A plain filename search, AI off.
      await openSearchCaptioned(page)
      await hold(500)
      await typeCaptioned(SEARCH_QUERY, (text) => typeSlowly(page, SEARCH_INPUT, text, 110))
      await page.waitForSelector(`${SEARCH_OVERLAY} .result-row`, waitBudget(30000))
      await hold(2600)
      await closeSearchCaptioned(page)
      await hold(600)

      // Settings > Provider: Local LLM, and the model starts.
      await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
      await hold(1300)
      expect(await pointerClick(settings, localOption)).toBe('clicked')
      await waitForLocalAiServer(page, true)
      await hold(1600)
      await invoke(page, 'plugin:window|set_focus', { label: 'main' })
      await hold(700)

      // The same dialog, now with "Ask anything".
      await openSearchCaptioned(page)
      await hold(500)
      if ((await getActiveMode(page)) !== 'ai') {
        await pressCaptioned(page, 'Meta+1')
        await expect.poll(() => getActiveMode(page), { timeout: waitBudget(5000) }).toBe('ai')
      }
      await hold(700)
      await typeCaptioned(AI_QUERY, (text) => typeSlowly(page, SEARCH_INPUT, text, 95))
      await hold(500)
      await pressCaptioned(page, 'Enter')
      await waitForTaxAnswer(page)
      await hold(1000)
      take.markPoster()
      await hold(3000)
      await closeSearchCaptioned(page)
      await hold(800)
    })
  } finally {
    await closeSearchDialog(page).catch(() => {})
    // Off again, so no other clip shows the AI chip or keeps the model running.
    await setSetting(page, 'ai.provider', 'off')
    await closeScopedWindow(page, settings, 'settings')
  }
}

/**
 * Settings > Appearance > Language, the list open, ArrowDown through a dozen languages:
 * the picker previews each one as it's highlighted, so the whole window re-renders.
 */
export async function languageSwitch(page: TauriPage, theme: ClipTheme): Promise<void> {
  const settings = await openStagedSettings(page, theme)
  try {
    const trigger = `document.getElementById('setting-appearance.language')?.querySelector('.select-trigger')`
    const found = await expect
      .poll(() => settings.evaluate<boolean>(`!!(${trigger})`), { timeout: waitBudget(5000) })
      .toBe(true)
      .then(
        () => true,
        () => false,
      )
    if (!found) {
      const seen = await settings.evaluate<string>(
        `JSON.stringify({ selected: document.querySelector('.section-item.selected')?.textContent?.trim(), rows: [...document.querySelectorAll('.setting-row')].map((r) => r.id).slice(0, 12) })`,
      )
      throw new Error(`the language picker isn't on screen: ${seen}`)
    }

    const take = await recordWindow(page, { slug: 'language-switch', theme, label: 'settings' })
    await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
    await inTake(take, async () => {
      await hold(800)
      take.markStart()
      await hold(LEAD_IN_MS)
      expect(await pointerClick(settings, trigger)).toBe('clicked')
      await expect
        .poll(
          () => settings.evaluate<boolean>(`!!(${selectContentExpr(trigger)})?.querySelector('[data-part="item"]')`),
          {
            timeout: waitBudget(5000),
          },
        )
        .toBe(true)
      await hold(700)
      for (let i = 0; i < 12; i++) {
        await pressCaptioned(settings, 'ArrowDown')
        await hold(650)
        if (i === 5) take.markPoster()
      }
      // Ends with the list still open on the last language.
      await hold(500)
    })
    // Off camera: pick English in the open list, from the settings window itself.
    await pickEnglish(settings, trigger)
  } finally {
    await setSetting(page, 'appearance.language', 'en')
    await closeScopedWindow(page, settings, 'settings')
  }
}

/**
 * Picks English in the language picker's OPEN list, the way a person would, and waits for
 * the window to read English.
 *
 * ❗ Through the picker in the settings window, ❌ never `setSetting` from the main window
 * and ❌ never by closing the list: closing it commits whatever is highlighted, and a
 * main-window reset didn't stick, so a take that ended on Vietnamese left it saved, and
 * every later window (the viewer) opened in Vietnamese (first real run, 2026-10-22).
 */
export async function pickEnglish(settings: TauriPage, trigger: string): Promise<void> {
  // Two picks, British English then English, because a pick of the value that's already
  // saved changes nothing: after previewing a dozen languages the saved value is still
  // `en`, so picking `en` alone left the backend on the last PREVIEWED one, and the
  // next viewer window opened in Vietnamese (first real run, 2026-10-22).
  for (const tag of ['en-GB', 'en']) {
    const item = `${selectContentExpr(trigger)}?.querySelector('[data-part="item"][data-value="${tag}"]')`
    // The menu stays mounted while closed, so "open" is its `data-state`, not its presence.
    const open = `${selectContentExpr(trigger)}?.getAttribute('data-state') === 'open'`
    await expect
      .poll(
        async () => {
          if (await settings.evaluate<boolean>(`!!(${item}) && ${open}`)) return true
          await pointerClick(settings, trigger)
          return false
        },
        { timeout: waitBudget(5000) },
      )
      .toBe(true)
    expect(await pointerClick(settings, item)).toBe('clicked')
    await expect
      .poll(() => settings.evaluate<string>(`document.documentElement.lang`), { timeout: waitBudget(10000) })
      .toBe(tag)
  }
}

/**
 * Opens the settings window on Appearance > Colors and formats, in English and `theme`.
 *
 * The theme is picked in the settings window's OWN theme control. Set from the main window
 * instead (`setTheme`), the settings window rendered dark while its Theme mode control
 * still showed Light (first real run, 2026-10-22).
 */
export async function openStagedSettings(page: TauriPage, theme: ClipTheme): Promise<TauriPage> {
  const settings = await openSettingsWindowViaProd(page)
  await settings.waitForSelector('.settings-sidebar', waitBudget(5000))
  // Appearance > Colors and formats, by position: the sidebar's first item is the bare
  // "Appearance" group (no rows of its own), its first child the section with the
  // language and theme rows. By position because a previous take may have left the
  // window in another language.
  await expect
    .poll(
      () =>
        settings.evaluate<boolean>(`(function () {
          document.querySelectorAll('.section-item')[1]?.click();
          return !!document.getElementById('setting-theme.mode') && !!document.getElementById('setting-appearance.language');
        })()`),
      { timeout: waitBudget(10000) },
    )
    .toBe(true)
  if ((await settings.evaluate<string>(`document.documentElement.lang`)) !== 'en') {
    const trigger = `document.getElementById('setting-appearance.language')?.querySelector('.select-trigger')`
    expect(await pointerClick(settings, trigger)).toBe('clicked')
    await pickEnglish(settings, trigger)
  }
  const label = theme === 'dark' ? 'Dark' : 'Light'
  const option = `[...document.getElementById('setting-theme.mode').querySelectorAll('button, [data-part="item"]')].find((b) => b.textContent.trim() === ${JSON.stringify(label)})`
  expect(await pointerClick(settings, option)).toBe('clicked')
  await expect
    .poll(() => settings.evaluate<boolean>(`window.matchMedia('(prefers-color-scheme: dark)').matches`), {
      timeout: waitBudget(10000),
    })
    .toBe(theme === 'dark')
  return settings
}

/**
 * The sections the settings pan visits, by their English sidebar labels: a spread across
 * the groups, skipping the group headers themselves, which hold no rows.
 */
const PAN_SECTIONS = [
  'Zoom and density',
  'File and folder sizes',
  'Navigation & file ops',
  'Drive indexing',
  'Provider',
  'Ask Cmdr',
  'SMB/Network shares',
]

/** A short pan through Settings sections. */
export async function settingsPan(page: TauriPage, theme: ClipTheme): Promise<void> {
  const settings = await openStagedSettings(page, theme)
  try {
    const take = await recordWindow(page, { slug: 'settings', theme, label: 'settings' })
    await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)
      for (const [i, section] of PAN_SECTIONS.entries()) {
        const clicked = await settings.evaluate<boolean>(`(function () {
          var item = [...document.querySelectorAll('.section-item')].find((s) => s.textContent.trim() === ${JSON.stringify(section)});
          if (item) item.click();
          return !!item;
        })()`)
        if (!clicked) throw new Error(`no Settings section reads '${section}'`)
        await hold(1100)
        if (i === 3) take.markPoster()
      }
    })
  } finally {
    await closeScopedWindow(page, settings, 'settings')
  }
}

/**
 * Opens a viewer window on `path` at the clip's size, and returns it with its CGWindowID.
 * The id is read while the window is a point taller than `CLIP_VIEWER`: the two viewer
 * windows end up the same size, and `requireWindowId` tells windows apart by size.
 */
async function openClipViewer(
  page: TauriPage,
  path: string,
): Promise<{ viewer: TauriPage; label: string; id: number }> {
  const viewer = await openViewerWindow(page, path)
  const label = viewer.targetWindow ?? ''
  const sizeTo = async (height: number): Promise<void> => {
    await setWindowSize(page, label, CLIP_VIEWER.width, height)
    await expect
      .poll(async () => (await windowMetrics(page, label)).logical.height, { timeout: waitBudget(5000) })
      .toBe(height)
  }
  await sizeTo(CLIP_VIEWER.height + 1)
  // Polled: right after a resize the window server can still list the old frame, or a
  // frame 2 points bigger (seen once, 2026-10-26), though the webview already reports the new one.
  let id = 0
  await expect
    .poll(
      () => {
        try {
          id = requireWindowId(appPid(), { width: CLIP_VIEWER.width, height: CLIP_VIEWER.height + 1 }, label)
          return true
        } catch {
          return false
        }
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  await sizeTo(CLIP_VIEWER.height)
  return { viewer, label, id }
}

/**
 * Moves `label`'s window onto `anchor`'s exact position, and re-applies the move until
 * both report the same origin: a viewer window may still be placing itself just after it
 * opens.
 */
async function stackOn(page: TauriPage, label: string, anchor: string): Promise<void> {
  const l = JSON.stringify(label)
  const a = JSON.stringify(anchor)
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`(async function() {
          var invoke = window.__TAURI_INTERNALS__.invoke;
          var target = await invoke('plugin:window|outer_position', { label: ${a} });
          var now = await invoke('plugin:window|outer_position', { label: ${l} });
          if (now.x === target.x && now.y === target.y) return true;
          await invoke('plugin:window|set_position', { label: ${l}, value: { Physical: { x: target.x, y: target.y } } });
          return false;
        })()`),
      { timeout: waitBudget(5000), intervals: [300] },
    )
    .toBe(true)
}

/** The view mode picker's label in a viewer's toolbar: `Image (0)`, `Text (1)`, `Hex (3)`, … */
function viewerModeLabel(viewer: TauriPage): Promise<string> {
  return viewer.evaluate<string>(
    `(document.querySelector('.select-trigger[aria-label="View mode"]')?.textContent ?? '').trim()`,
  )
}

/**
 * Presses a view key (`0`–`3`) until the toolbar's picker reads `label`. The viewer drops
 * a mode key while it's still loading, so a single press right after a file opens can
 * land on nothing.
 */
async function switchViewerMode(viewer: TauriPage, digit: '0' | '1' | '2' | '3', label: string): Promise<void> {
  // One caption however many presses it takes: the re-presses are for a viewer still loading.
  captionKey(digit)
  await expect
    .poll(
      async () => {
        if ((await viewerModeLabel(viewer)).startsWith(label)) return true
        await pressKey(viewer, digit, `Digit${digit}`)
        return false
      },
      { timeout: waitBudget(10000), intervals: [400] },
    )
    .toBe(true)
}

/** True once every image in `selector` has decoded, so a take never shows one filling in. */
function imagesLoaded(viewer: TauriPage, selector: string): Promise<boolean> {
  return viewer.evaluate<boolean>(
    `(function(){ var imgs = [...document.querySelectorAll(${JSON.stringify(selector)})]; return imgs.length > 0 && imgs.every(function (i) { return i.complete && i.naturalWidth > 0 }) })()`,
  )
}

/**
 * A photo, then the same photo as binary (2) and hex (3); then a Markdown file as plain
 * text, then formatted (0).
 *
 * Two viewer windows, stacked on one frame: the Markdown one behind, already in Text
 * view, and the photo's in front. A viewer window can only reach another file through a
 * Markdown link, and one reached that way opens formatted, so raising the second window
 * is how the clip moves from the photo's hex to the Markdown's text. The take records the
 * photo's window with the Markdown's included.
 */
export async function viewer(page: TauriPage, theme: ClipTheme): Promise<void> {
  await setTheme(page, theme)
  const folder = join(DEMO_ROOT, DEMO.fieldNotes)
  const doc = await openClipViewer(page, join(folder, VIEWER_DOC))
  let photo: Awaited<ReturnType<typeof openClipViewer>> | null = null
  try {
    await doc.viewer.waitForSelector('.markdown-body', waitBudget(10000))
    // Off camera: Text view, what the Markdown window shows when it comes up.
    await switchViewerMode(doc.viewer, '1', 'Text')

    const shown = await openClipViewer(page, join(folder, ARTWORK.file))
    photo = shown
    await expect.poll(() => imagesLoaded(shown.viewer, '.media-image'), { timeout: waitBudget(10000) }).toBe(true)
    await stackOn(page, shown.label, doc.label)
    // The Markdown window right behind the photo's, both above every other window.
    await invoke(page, 'plugin:window|set_focus', { label: doc.label })
    await invoke(page, 'plugin:window|set_focus', { label: shown.label })

    focusApp(appPid())
    const take = await startTake({ slug: 'viewer', theme, windowId: shown.id, includeWindowIds: [doc.id] })
    await invoke(page, 'plugin:window|set_focus', { label: shown.label })
    await inTake(take, async () => {
      await hold(600)
      take.markStart()
      await hold(1200)
      take.markPoster()
      await hold(2800)

      await switchViewerMode(shown.viewer, '2', 'Binary')
      await shown.viewer.waitForSelector('.raw-byte-view .raw-row', waitBudget(10000))
      await hold(3200)
      await switchViewerMode(shown.viewer, '3', 'Hex')
      await shown.viewer.waitForSelector('.raw-byte-view .raw-hex', waitBudget(10000))
      await hold(3400)

      await invoke(page, 'plugin:window|set_focus', { label: doc.label })
      await hold(3600)
      await switchViewerMode(doc.viewer, '0', 'Markdown')
      await doc.viewer.waitForSelector('.markdown-body', waitBudget(10000))
      await expect.poll(() => imagesLoaded(doc.viewer, '.markdown-body img'), { timeout: waitBudget(10000) }).toBe(true)
      await hold(4400)
    })
  } finally {
    if (photo !== null) await closeScopedWindow(page, photo.viewer, photo.label)
    await closeScopedWindow(page, doc.viewer, doc.label)
  }
}
