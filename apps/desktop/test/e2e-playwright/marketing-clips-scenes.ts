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
import { DEMO, DEMO_ROOT, QUEUE_SOURCES, SEARCH_QUERY, resetBackup, resetRawImports } from './marketing-clips-demo.js'
import { startTake } from './marketing-clips-take.js'
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
const CLIP_QUEUE = { width: 600, height: 300 }

const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'
const MULTI_RENAME = '[data-dialog-id="multi-rename"]'

/** Lets a frame sit on screen. Choreography only; see the file header. */
function hold(ms: number): Promise<void> {
  return sleep(ms)
}

/**
 * Types `text` into `selector` one character at a time, the way a person would, through
 * `input` events so the bound state follows. `perCharMs` is choreography.
 */
async function typeSlowly(page: TauriPage, selector: string, text: string, perCharMs = 85): Promise<void> {
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

async function invoke(page: TauriPage, command: string, args: Record<string, unknown>): Promise<void> {
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)
}

/** Per-file copy pace and per-item rollback pace (`src-tauri/src/test_mode.rs`); `null` clears. */
async function setThrottles(page: TauriPage, copyMs: number | null, rollbackMs: number | null): Promise<void> {
  await invoke(page, 'set_test_throttle', { ms: copyMs })
  await invoke(page, 'set_test_rollback_throttle', { ms: rollbackMs })
}

/** Cancels whatever is still running and waits for the operation lanes to empty. */
async function drainOperations(page: TauriPage): Promise<void> {
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
async function stageMain(page: TauriPage, theme: ClipTheme, left: string, right: string): Promise<void> {
  await setWindowSize(page, 'main', CLIP_MAIN.width, CLIP_MAIN.height)
  await expect
    .poll(async () => (await windowMetrics(page, 'main')).logical.width, { timeout: waitBudget(5000) })
    .toBe(CLIP_MAIN.width)
  await waitForIndexedSizes(page, LOG)
  await stageCosmetics(page)
  await resetTabs('left')
  await resetTabs('right')
  await setViewMode('left', 'full')
  await setViewMode('right', 'full')
  await mcpCall('nav_to_path', { pane: 'left', path: left })
  await mcpCall('nav_to_path', { pane: 'right', path: right })
  const leftFocused = await page.evaluate<boolean>(
    `document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') ?? false`,
  )
  if (!leftFocused) await mcpCall('switch_pane', {})
  await pinVolatileChrome(page)
  await setTheme(page, theme)
  await dismissAllToasts(page)
}

/**
 * Brings the app (and `label`'s window) to the front and starts a take of it.
 *
 * Front matters even though ScreenCaptureKit records a covered window: WKWebView starves
 * `requestAnimationFrame` in an occluded window, so a clip recorded behind the terminal
 * would show frozen progress bars.
 */
async function recordWindow(
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
async function inTake(take: Take, body: () => Promise<void>): Promise<void> {
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
      await dispatchMenuCommand(page, 'file.copy')
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
      await hold(1000)
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
      for (const [i, source] of QUEUE_SOURCES.entries()) {
        await startCopy(page, join(DEMO_ROOT, DEMO.projects, source), join(DEMO_ROOT, DEMO.backup))
        if (i === 2) take.markPoster()
        await hold(450)
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
      await hold(1200)
    })
  } finally {
    await setThrottles(page, null, null)
    await drainOperations(page)
    await closeScopedWindow(page, queue, 'queue')
    await dismissAllToasts(page)
    resetBackup()
  }
}

/** Starts a local copy over IPC, exactly what the F5 dialog dispatches, with no modal. */
async function startCopy(page: TauriPage, source: string, destination: string): Promise<void> {
  await invoke(page, 'copy_between_volumes', {
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
  })
}

/**
 * Sizes the queue window and parks it over the main window's lower right, inside the
 * main window's frame: the take is cropped to that frame, so anything outside it would
 * be cut off.
 */
async function placeQueueOverMain(page: TauriPage): Promise<void> {
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
    const take = await recordWindow(page, { slug: 'multi-rename', theme, label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await dispatchMenuCommand(page, 'file.multiRename')
      await page.waitForSelector(MULTI_RENAME, waitBudget(5000))
      await hold(900)
      await typeSlowly(page, `${MULTI_RENAME} input[aria-label="Name mask"]`, 'Iceland [Y-M-D] [C:3]')
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
    await dispatchMenuCommand(page, 'selection.deselectAll')
    await dismissAllToasts(page)
    resetRawImports()
  }
}

/** A query typed, results landing. */
export async function search(page: TauriPage, theme: ClipTheme): Promise<void> {
  await stageMain(page, theme, DEMO_ROOT, join(DEMO_ROOT, DEMO.documents))
  // Off camera: the dialog keeps the last query across opens, so clear it first.
  await openSearchDialog(page)
  await resetSearchDialog(page)
  await closeSearchDialog(page)
  try {
    const take = await recordWindow(page, { slug: 'search', theme, label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await openSearchDialog(page)
      await hold(500)
      await typeSlowly(page, SEARCH_INPUT, SEARCH_QUERY, 110)
      await page.waitForSelector(`${SEARCH_OVERLAY} .result-row`, waitBudget(30000))
      await hold(700)
      take.markPoster()
      await hold(2200)
    })
  } finally {
    await closeSearchDialog(page).catch(() => {})
  }
}

/**
 * Settings > Appearance > Language, the list open, ArrowDown through a dozen languages:
 * the picker previews each one as it's highlighted, so the whole window re-renders.
 */
export async function languageSwitch(page: TauriPage, theme: ClipTheme): Promise<void> {
  await setTheme(page, theme)
  const settings = await openSettingsWindowViaProd(page)
  try {
    await settings.waitForSelector('.settings-sidebar', waitBudget(5000))
    // The language row lives on the first section, Appearance > Colors and formats.
    await settings.evaluate(`document.querySelectorAll('.section-item')[0]?.click()`)
    const trigger = `document.getElementById('setting-appearance.language')?.querySelector('.select-trigger')`
    await expect.poll(() => settings.evaluate<boolean>(`!!(${trigger})`), { timeout: waitBudget(5000) }).toBe(true)

    const take = await recordWindow(page, { slug: 'language-switch', theme, label: 'settings' })
    await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
    await inTake(take, async () => {
      await hold(800)
      take.markStart()
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
        await pressKey(settings, 'ArrowDown')
        await hold(650)
        if (i === 5) take.markPoster()
      }
      // Closing without a pick leaves the committed language: back to English.
      expect(await pointerClick(settings, trigger)).toBe('clicked')
      await hold(1000)
    })
  } finally {
    await setSetting(page, 'appearance.language', 'en')
    await closeScopedWindow(page, settings, 'settings')
  }
}

/** A short pan through Settings sections. */
export async function settingsPan(page: TauriPage, theme: ClipTheme): Promise<void> {
  await setTheme(page, theme)
  const settings = await openSettingsWindowViaProd(page)
  try {
    await settings.waitForSelector('.section-item', waitBudget(5000))
    await settings.evaluate(`document.querySelectorAll('.section-item')[0]?.click()`)
    const sections = await settings.evaluate<number>(`document.querySelectorAll('.section-item').length`)
    const visits = Math.min(8, sections)

    const take = await recordWindow(page, { slug: 'settings', theme, label: 'settings' })
    await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
    await inTake(take, async () => {
      await hold(900)
      take.markStart()
      for (let i = 1; i < visits; i++) {
        await settings.evaluate(`document.querySelectorAll('.section-item')[${String(i)}]?.click()`)
        await hold(1100)
        if (i === 3) take.markPoster()
      }
      await hold(400)
    })
  } finally {
    await closeScopedWindow(page, settings, 'settings')
  }
}

/** One file after another in the viewer: Markdown, an image, text, then a binary as bytes and hex. */
export async function viewer(page: TauriPage, theme: ClipTheme): Promise<void> {
  await setTheme(page, theme)
  const notes = join(DEMO_ROOT, DEMO.fieldNotes)
  const viewerPage = await openViewerWindow(page, join(notes, 'README.md'))
  const label = viewerPage.targetWindow ?? ''
  try {
    await setWindowSize(page, label, CLIP_VIEWER.width, CLIP_VIEWER.height)
    await expect
      .poll(async () => (await windowMetrics(page, label)).logical.width, { timeout: waitBudget(5000) })
      .toBe(CLIP_VIEWER.width)
    await viewerPage.waitForSelector('.markdown-body img', waitBudget(10000))

    const take = await recordWindow(page, { slug: 'viewer', theme, label })
    await invoke(page, 'plugin:window|set_focus', { label })
    await inTake(take, async () => {
      await hold(600)
      take.markStart()
      await hold(1800)
      take.markPoster()

      await followLink(viewerPage, 'Full-size photo')
      await viewerPage.waitForSelector('.media-image', waitBudget(10000))
      await hold(1800)
      await goBack(viewerPage)
      await viewerPage.waitForSelector('.markdown-body', waitBudget(10000))
      await hold(500)

      await followLink(viewerPage, 'Packing list')
      await viewerPage.waitForSelector('.lines-container', waitBudget(10000))
      await hold(1800)
      await goBack(viewerPage)
      await viewerPage.waitForSelector('.markdown-body', waitBudget(10000))
      await hold(500)

      await followLink(viewerPage, 'Light controller firmware')
      await hold(600)
      await pressKey(viewerPage, '2', 'Digit2')
      await viewerPage.waitForSelector('.raw-byte-view', waitBudget(10000))
      await hold(1500)
      await pressKey(viewerPage, '3', 'Digit3')
      await hold(1800)
    })
  } finally {
    await closeScopedWindow(page, viewerPage, label)
  }
}

/** Clicks the Markdown link whose text is `text`; the viewer opens its target in this window. */
async function followLink(viewerPage: TauriPage, text: string): Promise<void> {
  const clicked = await viewerPage.evaluate<boolean>(`(function(){
      var links = document.querySelectorAll('.markdown-body a');
      for (var i = 0; i < links.length; i++) {
        if (links[i].textContent.trim() === ${JSON.stringify(text)}) { links[i].click(); return true; }
      }
      return false;
    })()`)
  if (!clicked) throw new Error(`the README has no link reading '${text}'`)
}

/** ⌘[ (`viewer.goBack`), the viewer's own history step. */
async function goBack(viewerPage: TauriPage): Promise<void> {
  await pressKey(viewerPage, 'Meta+[', 'BracketLeft')
}
