/**
 * Ask Cmdr surfaces beyond the first exchange, for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`): the rail's less common states, each on a thread of its own.
 *
 * The fake LLM's scripts carry most of them (`rail_fake_turn` in `chat/session.rs`):
 *
 *  - `ask-cmdr-every-tool`: `[every-tool]`, one turn calling every other read tool.
 *  - `ask-cmdr-rename-undo` and `-undo-partial`: `[rename]` stages a plan on the fixture tree, the
 *    real review applies it, and the thread shows its Undo, then an undo that couldn't restore
 *    everything because a file moved on since.
 *  - `ask-cmdr-errors`: `[fail:<kind>]`, each failure stacking one line in the thread.
 *
 * The rest stage what a turn can't:
 *
 *  - `ask-cmdr-decisions`: a suggestion's decisions arrive as `proposalDecided` events, which no
 *    turn owns and nothing in Rust listens to, so a synthetic event renders them for real.
 *  - `ask-cmdr-wake-digest`: a forced wake, the same way `ask-cmdr-wake.spec.ts` does it.
 *  - The threads panel's other states, and the provider notice under a stubbed slot status (the
 *    fake provider is always ready, which is what lets the composer send).
 */

import fs from 'node:fs'
import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import {
  dismissAllToasts,
  drainOperations,
  emitBackendEvent,
  ensureAppReady,
  forceAgentWake,
  getFixtureRoot,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpAwaitItem } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface } from './i18n-capture-helpers.js'
import { RAIL, closeRail, openRail, sendToRail, unlockChat } from './i18n-capture-ask-cmdr.js'
import { stubIpcAnswer } from './i18n-capture-ipc-stub.js'

const NEW_CHAT = `${RAIL} .header-actions [aria-label="New chat"]`

/** Starts a surface's sink, on the rail, on a thread with nothing in it. */
async function onFreshThread(main: TauriPage, label: string): Promise<void> {
  await captureCall(main, 'reset')
  await captureCall(main, 'setSurface', label)
  await captureCall<boolean>(main, 'enable')
  await main.evaluate(`document.querySelector('${NEW_CHAT}')?.click()`)
  await main.waitForSelector(`${RAIL} .empty .empty-title`, 5000)
}

async function openChat(main: TauriPage): Promise<void> {
  await ensureAppReady(main)
  await openRail(main)
  await unlockChat(main)
}

/** Opens the threads panel from the rail header. */
async function openThreads(main: TauriPage): Promise<void> {
  await main.evaluate(`document.querySelector('${RAIL} [aria-label="Chats"]')?.click()`)
  await main.waitForSelector(`${RAIL} .sessions`, 5000)
}

/** Waits for a plain message's reply, which has no stats line (that's a tool-running turn's). */
async function waitForReply(main: TauriPage): Promise<void> {
  await expect
    .poll(() => main.evaluate<string>(`document.querySelector('${RAIL}')?.textContent || ''`), {
      timeout: waitBudget(15000),
    })
    .toContain('test assistant')
}

/** Opens every tool group the turn's end closed, so its lines are in the shot. */
async function openToolGroups(main: TauriPage): Promise<void> {
  await expect
    .poll(
      () =>
        main.evaluate<number>(`(function(){
          document.querySelectorAll('${RAIL} .group-toggle[aria-expanded="false"]')
            .forEach(function(toggle){ toggle.click(); });
          return document.querySelectorAll('${RAIL} .group-toggle[aria-expanded="false"]').length;
        })()`),
      { timeout: waitBudget(3000) },
    )
    .toBe(0)
}

/** One turn through every read tool, so each line's words render with its subject. */
export async function captureAskCmdrEveryTool(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await openChat(main)
  await captureSurface('ask-cmdr-every-tool', report, failed, async () => {
    await onFreshThread(main, 'ask-cmdr-every-tool')
    await sendToRail(main, `[every-tool] ${getFixtureRoot()}/left`)
    await main.waitForSelector(`${RAIL} .msg .turn-stats`, waitBudget(15000))
    await openToolGroups(main)
    return { page: main, readySelector: `${RAIL} .msg .turn-stats` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await closeRail(main).catch(() => {})
}

/**
 * The rename plan's aftermath: the Undo a finished batch leaves in the thread, then an undo that
 * couldn't finish because one of the files changed name again on disk. Renames real fixture files,
 * so it starts and ends on a fresh tree.
 */
export async function captureAskCmdrRename(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const left = `${getFixtureRoot()}/left`
  const undoButton = `${RAIL} .rename-applied button.undo`
  recreateFixtures(getFixtureRoot())
  await openChat(main)
  // The plan is checked against the backend's mirror of the focused pane, which a harness reset
  // leaves behind (`DETAILS.md` § "Claiming a pane's focus inside a spec"), so tell it directly and
  // wait for its listing to catch up with the fresh tree.
  await main.evaluate(`window.__TAURI_INTERNALS__.invoke('update_focused_pane', { pane: 'left' })`)
  await initMcpClient(main)
  await mcpAwaitItem('left', 'file-a.txt')
  await captureSurface('ask-cmdr-rename-undo', report, failed, async () => {
    await onFreshThread(main, 'ask-cmdr-rename-undo')
    await sendToRail(main, `[rename] ${left}`)
    // The review opens at the turn's end, and its Rename button waits for the preflight.
    const rename = `[data-dialog-id="bulk-rename-review"] .modal-footer button[aria-label^="Rename"]:not([disabled])`
    await main.waitForSelector(rename, waitBudget(15000))
    await main.evaluate(`document.querySelector(${JSON.stringify(rename)})?.click()`)
    await main.waitForSelector(undoButton, waitBudget(15000))
    // The rename's own completion toast would be a stray in this shot of the thread.
    await drainOperations(main)
    await dismissAllToasts(main)
    return { page: main, readySelector: undoButton }
  })
  await captureCall(main, 'disable').catch(() => {})

  await captureSurface('ask-cmdr-rename-undo-partial', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-rename-undo-partial')
    await captureCall<boolean>(main, 'enable')
    await drainOperations(main)
    // Gone from its new name, so undo restores the other file and reports this one as skipped.
    fs.rmSync(`${left}/notes-a.txt`)
    await main.evaluate(`document.querySelector(${JSON.stringify(undoButton)})?.click()`)
    await main.waitForSelector(`${RAIL} .rename-applied .rename-note`, waitBudget(15000))
    await drainOperations(main)
    await dismissAllToasts(main)
    return { page: main, readySelector: `${RAIL} .rename-applied .rename-note` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await drainOperations(main)
  recreateFixtures(getFixtureRoot())
  await closeRail(main).catch(() => {})
}

/** The provider failures a call can end in, stacked in one thread so they share an image. */
export async function captureAskCmdrErrors(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const kinds = ['noKey', 'notConfigured', 'unavailable', 'timeout', 'authFailed', 'rateLimited', 'provider']
  await openChat(main)
  await captureSurface('ask-cmdr-errors', report, failed, async () => {
    await onFreshThread(main, 'ask-cmdr-errors')
    for (const [at, kind] of kinds.entries()) {
      await sendToRail(main, `[fail:${kind}]`)
      await expect
        .poll(() => main.evaluate<number>(`document.querySelectorAll('${RAIL} .msg.error').length`), {
          timeout: waitBudget(5000),
        })
        .toBe(at + 1)
    }
    return { page: main, readySelector: `${RAIL} .msg.error` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await closeRail(main).catch(() => {})
}

/** A thread's decided suggestions, one row per way a decision can read. */
export async function captureAskCmdrDecisions(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const left = `${getFixtureRoot()}/left`
  const rows = [
    { verb: 'trash', what: left, ops: 12, outcome: { kind: 'rejected' } },
    { verb: 'move', what: left, ops: 3, outcome: { kind: 'running' } },
    { verb: 'copy', what: left, ops: 40, outcome: { kind: 'ran', done: 38, skipped: 1, failed: 1 } },
    { verb: 'delete', what: left, ops: 2, outcome: { kind: 'interrupted' } },
    { verb: 'rename', what: left, ops: 5, outcome: { kind: 'rejected' } },
    { verb: 'compress', what: left, ops: 7, outcome: { kind: 'rejected' } },
    { verb: 'extract', what: left, ops: 1, outcome: { kind: 'rejected' } },
    { verb: 'addServer', what: 'nas.local', ops: 0, outcome: { kind: 'saved', note: 'none' } },
    { verb: 'editServer', what: 'nas.local', ops: 0, outcome: { kind: 'saved', note: 'shareNotSaved' } },
    { verb: 'addServer', what: 'cloud.example.com', ops: 0, outcome: { kind: 'rejected' } },
  ]
  await openChat(main)
  await captureSurface('ask-cmdr-decisions', report, failed, async () => {
    await onFreshThread(main, 'ask-cmdr-decisions')
    await sendToRail(main, 'What is waiting on me?')
    await waitForReply(main)
    const conversationId = await main.evaluate<number>(`(async function(){
      var rows = await window.__TAURI_INTERNALS__.invoke('ask_cmdr_list_conversations', { limit: 1, offset: 0, includeArchived: false });
      return rows[0].id;
    })()`)
    for (const [seq, decision] of rows.entries()) {
      await emitBackendEvent(main, 'ask-cmdr-turn', {
        conversationId,
        run: null,
        event: { type: 'proposalDecided', messageId: 1_000_000 + seq, seq, decision },
      })
    }
    await expect
      .poll(() => main.evaluate<number>(`document.querySelectorAll('${RAIL} .decisions li').length`), {
        timeout: waitBudget(5000),
      })
      .toBe(rows.length)
    return { page: main, readySelector: `${RAIL} .decisions li` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await closeRail(main).catch(() => {})
}

/** A wake's thread, with what it noticed unfolded. */
export async function captureAskCmdrWakeDigest(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const folder = `wake-capture-${String(Date.now())}`
  await openChat(main)
  await captureSurface('ask-cmdr-wake-digest', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-wake-digest')
    await captureCall<boolean>(main, 'enable')
    await forceAgentWake(main, `/Users/e2e/${folder}`)
    // The digest is written once the first reply lands, and a thread opened before that shows
    // none (`src/lib/ask-cmdr/DETAILS.md` § "What a wake's thread opens with"), so keep reopening
    // it until it does. Going through the panel each time is what reloads the thread.
    await expect
      .poll(
        async () => {
          await openThreads(main)
          await main.evaluate(`[...document.querySelectorAll('${RAIL} .sessions .row')]
            .find(function(row){ return (row.textContent || '').includes(${JSON.stringify(folder)}); })?.click()`)
          return main.evaluate<boolean>(`document.querySelector('${RAIL} .wake-digest') !== null`)
        },
        { timeout: waitBudget(20000) },
      )
      .toBe(true)
    await main.evaluate(`document.querySelector('${RAIL} .wake-digest .digest-toggle')?.click()`)
    await expect
      .poll(() => main.evaluate<string>(`document.querySelector('${RAIL} .wake-digest')?.textContent || ''`), {
        timeout: waitBudget(5000),
      })
      .toContain(folder)
    return { page: main, readySelector: `${RAIL} .wake-digest` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await closeRail(main).catch(() => {})
}

/**
 * The threads panel's other faces: a search with no match, the archived list, and a row being
 * renamed. Works on a thread it starts itself, and leaves it unarchived so the shard's other Ask
 * Cmdr specs see the list they expect.
 */
export async function captureAskCmdrSessionStates(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const search = `${RAIL} .sessions .search-row input`
  await openChat(main)
  await main.evaluate(`document.querySelector('${NEW_CHAT}')?.click()`)
  await main.waitForSelector(`${RAIL} .empty .empty-title`, 5000)
  await sendToRail(main, 'A thread to archive')
  await waitForReply(main)
  await openThreads(main)

  await captureSurface('ask-cmdr-sessions-no-results', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-sessions-no-results')
    await captureCall<boolean>(main, 'enable')
    await main.evaluate(`(function(){
      var field = document.querySelector('${search}');
      field.value = 'zzz nothing has this in it';
      field.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
    await main.waitForSelector(`${RAIL} .sessions-body p.notice`, waitBudget(5000))
    return { page: main, readySelector: `${RAIL} .sessions-body p.notice` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await main.evaluate(`(function(){
    var field = document.querySelector('${search}');
    field.value = '';
    field.dispatchEvent(new Event('input', { bubbles: true }));
  })()`)
  await main.waitForSelector(`${RAIL} .sessions .conversation`, waitBudget(5000))

  await captureSurface('ask-cmdr-sessions-rename', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-sessions-rename')
    await captureCall<boolean>(main, 'enable')
    await main.evaluate(`document.querySelector('${RAIL} .sessions .conversation [aria-label="Rename"]')?.click()`)
    await main.waitForSelector(`${RAIL} .sessions .conversation input`, waitBudget(5000))
    return { page: main, readySelector: `${RAIL} .sessions .conversation input` }
  })
  await captureCall(main, 'disable').catch(() => {})
  // Leaving the field commits the same title, which closes the editor without changing a thing.
  await main.evaluate(`document.querySelector('${RAIL} .sessions .conversation input')?.blur()`)
  await main.waitForSelector(`${RAIL} .sessions .conversation .row`, waitBudget(5000))

  await main.evaluate(`document.querySelector('${RAIL} .sessions .conversation [aria-label="Archive"]')?.click()`)
  await captureSurface('ask-cmdr-sessions-archived', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-sessions-archived')
    await captureCall<boolean>(main, 'enable')
    await main.evaluate(`document.querySelector('${RAIL} .sessions .archived-toggle')?.click()`)
    await main.waitForSelector(`${RAIL} .sessions .archived-badge`, waitBudget(5000))
    return { page: main, readySelector: `${RAIL} .sessions .archived-badge` }
  })
  await captureCall(main, 'disable').catch(() => {})
  await main.evaluate(`document.querySelector('${RAIL} .sessions .conversation [aria-label="Unarchive"]')?.click()`)
  await main.evaluate(`document.querySelector('${RAIL} .sessions .archived-toggle')?.click()`)
  await main.evaluate(`document.querySelector('${RAIL} .sessions-header button')?.click()`)
  await closeRail(main).catch(() => {})
}

/**
 * The yellow notice over a rail with no provider ready, and the composer's Send gate under it.
 * Replaces the card's `askCmdr.composer.providerOff`, which this notice took over from.
 */
export async function captureAskCmdrProviderOff(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)
  await closeRail(main).catch(() => {})
  const restore = await stubIpcAnswer(main, 'ask_cmdr_slot_status', {
    ok: { readiness: 'off', nextCall: null, baseline: null },
  })
  try {
    await captureSurface('ask-cmdr-provider-off', report, failed, async () => {
      await captureCall(main, 'reset')
      await captureCall(main, 'setSurface', 'ask-cmdr-provider-off')
      await captureCall<boolean>(main, 'enable')
      await openRail(main)
      await main.waitForSelector(`${RAIL} .provider-notice`, waitBudget(5000))
      return { page: main, readySelector: `${RAIL} .provider-notice` }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    await closeRail(main).catch(() => {})
    await restore()
  }
}
