/**
 * The app-side staging both marketing captures share: the stills (`marketing-shots.spec.ts`)
 * and the clips (`marketing-clips.spec.ts`). Settings changed the way the app's own MCP
 * server changes them, the theme, the drive-index gate, and the window-chrome pin.
 *
 * Everything here changes what the app SHOWS through real app paths, or paints over a
 * rendered value; nothing writes a license or fakes a state the app would act on.
 */

import { waitBudget } from './wait-budget.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { ensureMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'
import { expect } from './fixtures.js'
import { indexIsSettled, parsePaneTabs, parsePaneView } from './marketing-shots-state.js'

/**
 * The window title the masters carry.
 *
 * The capture instances hold no license, so the app computes `Cmdr – Personal use only`
 * (`licensing/app_status.rs::get_window_title`). The masters show the plain brand name
 * instead, which is a deliberate marketing choice, applied to the rendered title only:
 * the instance stays unlicensed, and nothing here mints or stores a license.
 */
const MASTER_TITLE = 'Cmdr'

/**
 * Changes a setting the way the app's own MCP server does: emit `mcp-set-setting` and
 * let the main window's bridge apply it (`settings/mcp-main-bridge.ts`).
 *
 * ❗ Not `invoke('set_setting')` — there is no such Tauri command; the MCP tool is a
 * round-trip THROUGH the frontend. And not a direct CSS or `@tauri-apps/api/app` poke
 * either: those change how the app looks without changing what it thinks, so the
 * settings master would photograph a radio button disagreeing with the window around it.
 */
export async function setSetting(page: TauriPage, settingId: string, value: unknown): Promise<void> {
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
       event: 'mcp-set-setting',
       payload: { requestId: 'marketing-capture', settingId: ${JSON.stringify(settingId)}, value: ${JSON.stringify(value)} }
     })`,
  )
}

/** Switches the app between dark and light through the real setting, not a CSS override. */
export async function setTheme(page: TauriPage, mode: 'dark' | 'light'): Promise<void> {
  await setSetting(page, 'theme.mode', mode)
  // The theme reaches the UI through Tauri's per-app theme API, so the honest readback
  // is the media query the stylesheet itself keys off, not a class we could set.
  await expect
    .poll(async () => page.evaluate<boolean>(`window.matchMedia('(prefers-color-scheme: dark)').matches`), {
      timeout: waitBudget(10000),
    })
    .toBe(mode === 'dark')
}

/**
 * The look of the file lists, set every run rather than seeded once.
 *
 * The capture data dirs are persistent, so a setting written at creation time can't be
 * changed later without deleting the instance — and these are exactly the ones a master
 * is judged on. The orchestrator's `seedSettingsIfNew` covers suppressions (analytics,
 * update toasts); anything VISIBLE in a shot belongs here, where every run re-applies it.
 */
export async function stageCosmetics(page: TauriPage): Promise<void> {
  // Rainbow size tiers: the color is most of what makes a file list read as a product
  // shot rather than a directory dump.
  await setSetting(page, 'appearance.sizeColors', 'rainbow')

  // The Size column, whose two settings are independent: `listing.sizeUnit` decides
  // whether a row reads `1.2 MB` or a raw byte count, `appearance.fileSizeFormat` only
  // picks the scale (SI, the default a new user sees). Both are staged here rather than
  // seeded because a data dir cloned from the production app arrives carrying its
  // owner's choices, which is how a round once shipped with the wrong scale.
  await setSetting(page, 'listing.sizeUnit', 'dynamic')
  await setSetting(page, 'appearance.fileSizeFormat', 'si')
}

/**
 * Waits until folder sizes are real numbers rather than hourglasses.
 *
 * ❗ Not cosmetic. While the drive index reconciles, every size cell shows an hourglass
 * and every folder size reads `≥`, which is what a whole round of unusable masters looks
 * like — it has happened. The orchestrator clones a warm index in to make this instant;
 * this is the gate that proves it worked.
 *
 * Two conditions, because `indexStatus` alone isn't the whole truth: a volume reads
 * `fresh` while a replay or an aggregation pass is still running, and that pass paints
 * the status corner's hourglass into the top-right of every master. So the pixels get a
 * vote — the corner must be empty too (`#lib/indexing/IndexingStatusIndicator.svelte`).
 */
export async function waitForIndexedSizes(page: TauriPage, logPrefix: string): Promise<void> {
  await ensureMcpClient(page)
  let announced = false
  await expect
    .poll(
      async () => {
        const settled =
          indexIsSettled(await mcpReadResource('cmdr://state?include=volumes')) &&
          !(await page.evaluate<boolean>(`document.querySelector('.indexing-status') !== null`))
        if (!settled && !announced) {
          announced = true
          console.log(
            `${logPrefix} the drive index is still catching up. This happens when the copied index is ` +
              'more than ten million FSEvents behind, which on a busy machine means hours, not days. ' +
              'It takes about five minutes, once, and later runs reuse the result.',
          )
        }
        return settled
      },
      {
        // Long enough for a full reconcile of a ~6 M entry drive (measured 284 s for the
        // scan plus aggregation), with room for a loaded machine. ❌ Don't trim this to
        // "make the suite faster": the alternative to waiting is shipping hourglasses.
        timeout: waitBudget(900_000),
        intervals: [1000],
        message: 'the drive index never settled, so every folder size would photograph as an hourglass',
      },
    )
    .toBe(true)
}

/**
 * Pins the two bits of window chrome that photograph whatever the machine happens to be
 * doing: the title's license suffix, and the repo chip's ahead/behind/dirty state.
 *
 * Both are honest app output that makes a poor master. The title suffix depends on a
 * license the capture instance doesn't have, and the chip reports the working copy's real
 * unpushed-commit count, so the same shot says `main` one day and `main · +14` the next.
 *
 * ❗ This paints over the RENDERED value only. It writes no license, and it must never
 * grow into anything that changes what the app believes: a master is allowed to show a
 * chosen state, never a fake one the app would act on.
 *
 * A `MutationObserver` rather than a one-shot rewrite, because both are reactive: the
 * chip repaints whenever the repo's watcher fires (an agent committing in a sibling
 * worktree is enough), and the title refetches on every license event. Each pass writes
 * only what differs, so the observer settles instead of feeding itself.
 */
export async function pinVolatileChrome(page: TauriPage): Promise<void> {
  await page.evaluate(`(() => {
    if (window.__cmdrShotsChromePinned) return
    window.__cmdrShotsChromePinned = true
    const apply = () => {
      const title = document.querySelector('.title-text')
      if (title && title.textContent.trim() !== ${JSON.stringify(MASTER_TITLE)}) {
        title.textContent = ${JSON.stringify(MASTER_TITLE)}
      }
      for (const chip of document.querySelectorAll('.repo-chip')) {
        for (const state of ['dirty', 'ahead', 'behind', 'detached', 'unborn']) {
          if (chip.classList.contains(state)) chip.classList.remove(state)
        }
        if (chip.dataset.state !== 'clean') chip.dataset.state = 'clean'
        // The '·' separator and the '+14 / dirty' suffix are separate spans; dropping
        // them leaves the icon and the branch name, which is the clean-state chip.
        chip.querySelector('.sep')?.remove()
        chip.querySelector('.sub')?.remove()
      }
    }
    new MutationObserver(apply).observe(document.body, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      attributeFilter: ['class', 'data-state'],
    })
    apply()
  })()`)
}

/**
 * Leaves a pane with exactly one unpinned tab.
 *
 * ❗ Unpin FIRST. `close` and `close_others` deliberately SKIP pinned tabs, so a data dir
 * that remembers yesterday's pinned tab ends up with three tabs in the pane and a shot of
 * a layout nobody asked for. This is why the reset reads the live tab list rather than
 * firing `close_others` and hoping.
 */
export async function resetTabs(pane: 'left' | 'right'): Promise<void> {
  const state = await mcpReadResource('cmdr://state?include=panes')
  for (const tab of parsePaneTabs(state, pane)) {
    if (tab.pinned) await mcpCall('tab', { pane, action: 'set_pinned', tabId: tab.id, pinned: false })
  }
  await mcpCall('tab', { pane, action: 'close_others' }).catch(() => {
    // A single-tab pane has nothing to close, and says so rather than succeeding.
  })
}

/**
 * Puts a pane in a view mode, skipping the call when it's already there.
 *
 * ❗ The skip is required, not an optimization. `set_view_mode` acks on the pane's state
 * generation advancing, so setting brief on an already-brief pane never acks and fails
 * the whole run 1.5 s later with a message about a stalled frontend.
 */
export async function setViewMode(pane: 'left' | 'right', mode: 'full' | 'brief'): Promise<void> {
  const current = parsePaneView(await mcpReadResource('cmdr://state?include=panes'), pane)
  if (current === mode) return
  await mcpCall('set_view_mode', { pane, mode })
}
