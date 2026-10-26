/**
 * The long-form footage for the 90-second demo video (issue #693): seven beats, dark
 * theme only, recorded as ProRes masters for an editor rather than encoded for the web.
 *
 * Same machinery as the website clips (`marketing-clips-scenes.ts`): the staged demo
 * tree, the take recorder, real UI actions. The differences are the pace (a confident
 * human demo: a beat between actions, keys at human speed), the length (12–20 s of usable
 * footage per beat), a bigger, zoomed window so text reads on a phone, and NOTES: every
 * key moment is named on the take, and `pnpm marketing:clips --video` lists them in
 * `video-beats.json` for the editor.
 *
 * ❗ `hold()` is choreography, as in the clips; every wait on the app is a real probe.
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
  ensureExplorerFocused,
  pointerClick,
  pressKey,
  selectContentExpr,
} from './helpers.js'
import { clickTransferStart, confirmRollback } from './conflict-helpers.js'
import {
  SEARCH_INPUT,
  SEARCH_OVERLAY,
  closeSearchDialog,
  getActiveMode,
  openSearchDialog,
  pressMetaDigit,
  resetSearchDialog,
} from './search-helpers.js'
import { setSetting } from './marketing-stage.js'
import {
  drainOperations,
  hold,
  inTake,
  invoke,
  openStagedSettings,
  pickEnglish,
  recordWindow,
  resetMultiRenameMask,
  setThrottles,
  stageMain,
  typeSlowly,
} from './marketing-clips-scenes.js'
import { AI_QUERY, DEMO, DEMO_ROOT, SENSOR_LOG_QUERY, resetBackup, resetCameraRoll } from './marketing-clips-demo.js'
import type { Take } from './marketing-clips-take.js'

/**
 * The video's main-window look: bigger than the clips, at 125% zoom, so a 1080p export
 * still reads on a phone.
 */
export const VIDEO_LOOK = { size: { width: 1280, height: 800 }, zoom: 125 } as const

/** Slack recorded before the first action and after the last, for the editor's cuts. */
const SLACK_MS = 1500

const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'
const MULTI_RENAME = '[data-dialog-id="multi-rename"]'

/** Opens the entry under the cursor named `name`, the way a person does: cursor there, Enter. */
export async function openEntry(page: TauriPage, pane: 'left' | 'right', name: string): Promise<void> {
  await mcpCall('move_cursor', { pane, filename: name })
  await hold(450)
  await ensureExplorerFocused(page)
  await page.keyboard.press('Enter')
}

/**
 * Up one folder with ⌘↑ (`nav.parent`), the key a person presses. Not by Enter on `..`
 * (inside a zip and the `.git` portal there's no `..` row), and ❗ not by dispatching
 * `nav.parent` as a command: that went up TWO levels per dispatch (logged twice,
 * 2026-10-23), a possible app bug reported with this run.
 */
export async function goParent(page: TauriPage): Promise<void> {
  await ensureExplorerFocused(page)
  await pressKey(page, 'Meta+ArrowUp')
}

/**
 * Answers the first-time "how should Cmdr open this archive?" menu with its highlighted
 * first row, "Browse like a folder", when it shows. Once answered the app may remember the
 * choice, so the menu's absence is fine too. Unanswered, the first take after a fresh
 * launch waited on it until it failed (2026-10-23).
 */
export async function chooseBrowseLikeAFolder(page: TauriPage): Promise<void> {
  const state = async (): Promise<'menu' | 'inside' | 'waiting'> =>
    page.evaluate<'menu' | 'inside' | 'waiting'>(`(function () {
      var rows = [...document.querySelectorAll('[data-menu-row]')];
      if (rows.some(function (r) { return (r.textContent || '').indexOf('Browse like a folder') !== -1; })) return 'menu';
      var listed = [...document.querySelectorAll('.file-pane.is-focused .file-entry')].some(function (e) {
        return (e.textContent || '').indexOf('Brand refresh') !== -1;
      });
      return listed ? 'inside' : 'waiting';
    })()`)
  await expect.poll(state, { timeout: waitBudget(15000) }).not.toBe('waiting')
  if ((await state()) !== 'menu') return
  await hold(1100)
  await pressKey(page, 'Enter')
}

/**
 * A browser-side predicate over a `.file-entry` element `e`: its name is `name`, or `name`
 * plus an extension. With `prefix`, any name starting with `name`.
 *
 * ❗ Not a bare `startsWith`: leaving `Brand refresh 01` for its parent, a wait for
 * `Brand refresh` passed at once on the row it was leaving, so the next `nav.parent` fired
 * in the same millisecond and the take went up two folders (#703).
 */
export function rowNamed(name: string, options: { prefix?: boolean } = {}): string {
  const n = JSON.stringify(name)
  const text = `(e.querySelector('.col-name')?.textContent ?? e.textContent ?? '').trim()`
  return options.prefix ? `${text}.startsWith(${n})` : `((t) => t === ${n} || t.startsWith(${n} + '.'))(${text})`
}

/** Waits for the focused pane to list `name` (see `rowNamed` for what matches). */
export async function waitForRow(page: TauriPage, name: string, options: { prefix?: boolean } = {}): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`[...document.querySelectorAll('.file-pane.is-focused .file-entry')].some((e) =>
          ${rowNamed(name, options)})`),
      { timeout: waitBudget(30000), message: `the focused pane never listed '${name}'` },
    )
    .toBe(true)
}

/** Waits for the focused pane to list at least `min` rows (`..` included). */
export async function waitForRowCount(page: TauriPage, min: number): Promise<void> {
  await expect
    .poll(() => page.evaluate<number>(`document.querySelectorAll('.file-pane.is-focused .file-entry').length`), {
      timeout: waitBudget(15000),
    })
    .toBeGreaterThanOrEqual(min)
}

/** Runs a beat's body between slack, start, and end markers. */
export async function beat(take: Take, body: () => Promise<void>): Promise<void> {
  await inTake(take, async () => {
    await hold(SLACK_MS)
    take.markStart()
    await body()
    take.markEnd()
    await hold(SLACK_MS)
  })
}

/** 1. A 50,000-file folder opens at once, scrolls fast, and a search finds one file instantly. */
export async function speed(page: TauriPage): Promise<void> {
  await stageMain(page, 'dark', join(DEMO_ROOT, 'Archive'), join(DEMO_ROOT, DEMO.photos), VIDEO_LOOK)
  await mcpCall('move_cursor', { pane: 'left', filename: 'Sensor logs' })
  await openSearchDialog(page)
  await resetSearchDialog(page)
  await closeSearchDialog(page)
  await dismissAllToasts(page)
  try {
    const take = await recordWindow(page, { slug: 'speed', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(1000)
      take.note('Enter on "Sensor logs", a folder of 50,000 files')
      await openEntry(page, 'left', 'Sensor logs')
      await waitForRow(page, 'sensor-00001')
      take.note('the 50,000-file listing is up')
      await hold(1500)
      take.note('scrolling fast, page by page')
      for (let i = 0; i < 10; i++) {
        await page.keyboard.press('PageDown')
        await hold(170)
      }
      await hold(500)
      take.note('End: straight to the last of 50,000')
      await page.keyboard.press('End')
      await hold(1300)
      await page.keyboard.press('Home')
      await hold(900)
      take.note('search opens')
      await openSearchDialog(page)
      await hold(500)
      await typeSlowly(page, SEARCH_INPUT, SENSOR_LOG_QUERY, 95)
      await page.waitForSelector(`${SEARCH_OVERLAY} .result-row`, waitBudget(30000))
      take.note('the one matching file is found')
      take.markPoster()
      await hold(2500)
      await closeSearchDialog(page)
    })
  } finally {
    await closeSearchDialog(page).catch(() => {})
  }
}

/** 2. A big copy with a live bar, Pause, Resume, then Rollback, the bar walking back. */
export async function trust(page: TauriPage): Promise<void> {
  resetBackup()
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.photos), join(DEMO_ROOT, DEMO.backup), VIDEO_LOOK)
  await mcpCall('move_cursor', { pane: 'left', filename: 'Iceland 2026' })
  // 64 photos at 170 ms each is an ~11 s copy, long enough for every control to read.
  await setThrottles(page, 170, 90)
  try {
    const take = await recordWindow(page, { slug: 'trust', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('F5: copy 64 photos (284 MB) to Backup')
      await dispatchMenuCommand(page, 'file.copy')
      await page.waitForSelector(TRANSFER_DIALOG, waitBudget(5000))
      await hold(1200)
      await clickTransferStart(page)
      await page.waitForSelector(PROGRESS_DIALOG, waitBudget(5000))
      take.note('copying, with the live progress bar and the file tree')
      await hold(2600)
      take.markPoster()
      take.note('Pause')
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Pause')
      await hold(2000)
      take.note('Resume')
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Resume')
      await hold(2200)
      take.note('Rollback, and its confirmation')
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Rollback')
      await page.waitForSelector('[data-dialog-id="rollback-confirmation"]', waitBudget(5000))
      await hold(1300)
      await confirmRollback(page)
      take.note('the bar walks back to zero')
      await expect
        .poll(async () => !(await page.isVisible(PROGRESS_DIALOG)), { timeout: waitBudget(30000) })
        .toBeTruthy()
      take.note('rolled back: Backup is empty again')
    })
  } finally {
    await setThrottles(page, null, null)
    await drainOperations(page)
    await dismissAllToasts(page)
    resetBackup()
  }
}

/** 3. Multi-rename on 30 photos: a pattern with date and counter, the preview filling, then apply. */
export async function rename(page: TauriPage): Promise<void> {
  resetCameraRoll()
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.cameraRoll), join(DEMO_ROOT, DEMO.trip), VIDEO_LOOK)
  await dispatchMenuCommand(page, 'selection.selectAll')
  try {
    await resetMultiRenameMask(page)
    const take = await recordWindow(page, { slug: 'rename', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('30 photos selected; Multi-rename opens')
      await dispatchMenuCommand(page, 'file.multiRename')
      await page.waitForSelector(MULTI_RENAME, waitBudget(5000))
      await hold(1500)
      take.note('typing the pattern: name, date, and a 3-digit counter')
      await typeSlowly(page, `${MULTI_RENAME} input[aria-label="Name mask"]`, 'Iceland [Y-M-D] [C:3]', 120)
      take.note('the live preview shows every new name')
      await hold(2500)
      take.markPoster()
      take.note('Rename')
      await page.evaluate(`(function(){
          var buttons = document.querySelectorAll(${JSON.stringify(`${MULTI_RENAME} button`)});
          for (var i = 0; i < buttons.length; i++) {
            if (buttons[i].textContent.trim().indexOf('Rename ') === 0 && !buttons[i].disabled) { buttons[i].click(); return; }
          }
          throw new Error('no enabled Rename button');
        })()`)
      await expect.poll(async () => !(await page.isVisible(MULTI_RENAME)), { timeout: waitBudget(15000) }).toBeTruthy()
      take.note('renamed: the list shows the new names')
      await hold(1500)
    })
  } finally {
    if (await page.isVisible(MULTI_RENAME)) await clickButtonByText(page, `${MULTI_RENAME} button`, 'Cancel')
    await dispatchMenuCommand(page, 'selection.deselectAll')
    await dismissAllToasts(page)
    resetCameraRoll()
  }
}

/** 4. A zip browsed like a folder, then a Git repo's history as folders: commits and branches. */
export async function reach(page: TauriPage): Promise<void> {
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.code), join(DEMO_ROOT, DEMO.projects), VIDEO_LOOK)
  // The `.git` folder is hidden; the portal shows its history once it's listed.
  await setSetting(page, 'listing.showHiddenFiles', true)
  await invoke(page, 'set_show_virtual_git_portal', { enabled: true })
  await waitForRow(page, 'website')
  try {
    const take = await recordWindow(page, { slug: 'reach', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(600)
      take.note('Enter on "Brand assets.zip": Cmdr asks how to open it')
      await openEntry(page, 'left', 'Brand assets.zip')
      await chooseBrowseLikeAFolder(page)
      await waitForRow(page, 'Brand refresh')
      take.note('inside the zip, like any folder')
      await hold(900)
      await openEntry(page, 'left', 'Brand refresh')
      await waitForRow(page, 'Brand refresh 01')
      take.note('the zipped files, listed with sizes and dates')
      await hold(1800)
      await goParent(page)
      await waitForRow(page, 'Brand refresh')
      await goParent(page)
      await waitForRow(page, 'website')
      await hold(500)
      take.note('Enter on the "website" Git repo')
      await openEntry(page, 'left', 'website')
      await waitForRow(page, '.git')
      await hold(900)
      take.note('Enter on .git: the history as folders')
      await openEntry(page, 'left', '.git')
      await waitForRow(page, 'commits')
      take.markPoster()
      await hold(1300)
      await openEntry(page, 'left', 'commits')
      take.note('commits, one folder each')
      await waitForRowCount(page, 5)
      await hold(2000)
      await goParent(page)
      await waitForRow(page, 'branches')
      await openEntry(page, 'left', 'branches')
      await waitForRow(page, 'redesign')
      take.note('branches: main and redesign')
      await hold(1600)
    })
  } finally {
    await setSetting(page, 'listing.showHiddenFiles', false)
  }
}

/** The local model's runtime facts the AI beat waits on (`get_ai_runtime_status`). */
interface LocalAiRuntime {
  serverRunning: boolean
  modelInstalled: boolean
}

async function localAiRuntime(page: TauriPage): Promise<LocalAiRuntime> {
  return page.evaluate<LocalAiRuntime>(`window.__TAURI_INTERNALS__.invoke('get_ai_runtime_status')`)
}

/**
 * Off camera: switches the instance to Cmdr's built-in local model and waits for its
 * server. On an instance without the model it downloads it first, through the same
 * `start_ai_download` that Settings > AI > Download calls (about 2 GB; 90 s on a fast
 * line, 2026-10-23). The model stays in the clips data dir for later runs, and an
 * interrupted download resumes. No API key anywhere.
 */
async function ensureLocalAi(page: TauriPage): Promise<void> {
  await setSetting(page, 'ai.provider', 'local')
  if (!(await localAiRuntime(page)).modelInstalled) {
    console.log('[marketing-video] downloading the local AI model into the clips instance…')
    // Fire and forget: the command resolves only when the download ends, far past one
    // `evaluate`'s budget. The runtime status is the probe.
    await page.evaluate(`(function () { window.__TAURI_INTERNALS__.invoke('start_ai_download'); return true })()`)
    await expect
      .poll(async () => (await localAiRuntime(page)).modelInstalled, {
        timeout: waitBudget(480000),
        intervals: [5000],
        message: "the local AI model didn't finish downloading; run the beat again to resume it",
      })
      .toBe(true)
  }
  await expect
    .poll(async () => (await localAiRuntime(page)).serverRunning, {
      timeout: waitBudget(90000),
      message: "the local AI server didn't start",
    })
    .toBe(true)
}

/**
 * 5. Search by meaning, on Cmdr's built-in local model: a plain sentence in "Ask anything"
 * mode becomes a name pattern AND a date range, and only last year's tax files come back,
 * out of three years of them and beside a decoy (`stageTaxes`). Real inference, nothing
 * scripted: the model samples (`--temp 0.6`) and once answered with the date range alone,
 * so a take whose results aren't all tax files fails and Playwright's retry records it
 * again (2026-10-23).
 */
export async function ai(page: TauriPage): Promise<void> {
  await stageMain(page, 'dark', DEMO_ROOT, join(DEMO_ROOT, DEMO.documents), VIDEO_LOOK)
  await ensureLocalAi(page)
  const lastYearsReport = `Tax report ${String(new Date().getUTCFullYear() - 1)}`
  try {
    await openSearchDialog(page)
    await resetSearchDialog(page)
    if ((await getActiveMode(page)) !== 'ai') await pressMetaDigit(page, 1)
    await expect.poll(() => getActiveMode(page), { timeout: waitBudget(5000) }).toBe('ai')
    await closeSearchDialog(page)
    await dismissAllToasts(page)
    const take = await recordWindow(page, { slug: 'ai', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('search opens in "Ask anything" mode, on the whole studio drive')
      await openSearchDialog(page)
      await hold(900)
      take.note(`typing a plain sentence: "${AI_QUERY}"`)
      await typeSlowly(page, SEARCH_INPUT, AI_QUERY, 110)
      await hold(700)
      take.note('Enter: the local model turns it into a name pattern and a date range')
      await pressKey(page, 'Enter')
      await expect
        .poll(
          async () => {
            const rows = await page.evaluate<string[]>(
              `[...document.querySelectorAll('${SEARCH_OVERLAY} .result-row')].map(function (r) { return r.textContent || '' })`,
            )
            return rows.some((r) => r.includes(lastYearsReport)) && rows.every((r) => r.includes('Tax '))
          },
          {
            timeout: waitBudget(30000),
            message: `the AI search didn't list only tax files, ${lastYearsReport} among them`,
          },
        )
        .toBe(true)
      take.note(`only last year's tax files come back (${lastYearsReport}), with the filters it chose`)
      await hold(1200)
      take.markPoster()
      await hold(3600)
      await closeSearchDialog(page)
    })
  } finally {
    await closeSearchDialog(page).catch(() => {})
    // Off again, so no other beat or clip shows the AI chip or keeps the model running.
    await setSetting(page, 'ai.provider', 'off')
  }
}

/** 6. The language list running through a few languages, back to English, then dark to light live. */
export async function care(page: TauriPage): Promise<void> {
  const settings = await openStagedSettings(page, 'dark')
  try {
    const trigger = `document.getElementById('setting-appearance.language')?.querySelector('.select-trigger')`
    const take = await recordWindow(page, { slug: 'care', theme: 'dark', label: 'settings' })
    await invoke(page, 'plugin:window|set_focus', { label: 'settings' })
    await beat(take, async () => {
      await hold(800)
      take.note('the Language picker opens')
      expect(await pointerClick(settings, trigger)).toBe('clicked')
      await expect
        .poll(
          () => settings.evaluate<boolean>(`!!(${selectContentExpr(trigger)})?.querySelector('[data-part="item"]')`),
          {
            timeout: waitBudget(5000),
          },
        )
        .toBe(true)
      await hold(900)
      take.note('ArrowDown: every language previews live')
      for (let i = 0; i < 7; i++) {
        await pressKey(settings, 'ArrowDown')
        await hold(850)
        if (i === 3) take.markPoster()
      }
      take.note('back to English')
      await pickEnglish(settings, trigger)
      await hold(1200)
      take.note('Theme mode: Light, live')
      const light = `[...document.getElementById('setting-theme.mode').querySelectorAll('button, [data-part="item"]')].find((b) => b.textContent.trim() === 'Light')`
      expect(await pointerClick(settings, light)).toBe('clicked')
      await expect
        .poll(() => settings.evaluate<boolean>(`window.matchMedia('(prefers-color-scheme: dark)').matches`), {
          timeout: waitBudget(10000),
        })
        .toBe(false)
      await hold(1800)
    })
  } finally {
    await setSetting(page, 'theme.mode', 'dark')
    await closeScopedWindow(page, settings, 'settings')
  }
}

/** 7. A calm main window, the cursor moving between panes: the intro and outro backdrop. */
export async function hero(page: TauriPage): Promise<void> {
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.trip), join(DEMO_ROOT, DEMO.documents), VIDEO_LOOK)
  const take = await recordWindow(page, { slug: 'hero', theme: 'dark', label: 'main' })
  await beat(take, async () => {
    await ensureExplorerFocused(page)
    take.note('calm: the cursor walks down the photos')
    for (let i = 0; i < 5; i++) {
      await hold(800)
      await page.keyboard.press('ArrowDown')
    }
    await hold(1000)
    take.note('Tab: over to the other pane')
    await page.keyboard.press('Tab')
    take.markPoster()
    for (let i = 0; i < 3; i++) {
      await hold(900)
      await page.keyboard.press('ArrowDown')
    }
    await hold(1000)
    take.note('Tab: back again')
    await page.keyboard.press('Tab')
    await hold(1500)
  })
}
