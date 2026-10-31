/**
 * Ask Cmdr surface captures for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * Ask Cmdr is the largest single uncoupled area in the catalog, and none of it is
 * reachable from a dialog trigger: the rail is a panel inside the main window
 * whose content depends on Ask Cmdr's switch, on a provider being willing to answer,
 * and on there being threads to list. So this module walks the rail the way
 * `ask-cmdr.spec.ts` does, capturing five states in the order the user meets them:
 *
 *  1. `ask-cmdr-gate-off`: the "Ask Cmdr is off" gate a fresh profile opens to.
 *  2. `ask-cmdr-empty`: switched on, a new chat with no messages yet.
 *  3. `ask-cmdr-chat`: one exchange, so the message chrome, the thinking line, and
 *     the cost footer render.
 *  4. `ask-cmdr-tools`: a turn that ran tools, with its groups open and one line
 *     expanded, so tool lines, group headers, the preview, and the stats line render.
 *  5. `ask-cmdr-sessions`: the threads panel over a thread that exists.
 *
 * The rail's less common states are their own steps: `i18n-capture-ask-cmdr-turns.ts`.
 *
 * The replies come from the scripted fake LLM, not a real provider: the capture
 * launch sets `CMDR_E2E_ASK_CMDR_FAKE=1`, which is the single source of truth for
 * both `resolve_agent_llm` (which answers) and the composer's provider gate (which
 * allows the send), so the two can't disagree. Without it the composer refuses to
 * send and the chat surface would photograph an empty thread.
 *
 * The gate surface is BEST-EFFORT: the switch lives in `settings.json` and persists
 * for the life of the data dir, so a second capture run against a warm dir opens
 * straight to the composer. It's captured when the gate shows and recorded as a
 * documented skip when it doesn't, rather than faking a screen the user has
 * already passed. The rail's other gate ("Cloud AI is off") needs `ai.provider` on
 * Cloud, which the fake provider never is, so its keys couple by representative
 * rule instead.
 */

import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import { ensureAppReady, dispatchMenuCommand, waitForAskCmdrOnInBackend } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface } from './i18n-capture-helpers.js'

export const RAIL = '.ask-cmdr-rail'

/** True once the rail is mounted (open), whatever it's showing inside. */
function railOpen(main: TauriPage): Promise<boolean> {
  return main.evaluate<boolean>(`document.querySelector('${RAIL}') !== null`)
}

/**
 * Opens the rail through the same `askCmdr.toggle` command the View menu and
 * ⌘⌥A use, re-dispatching inside the poll: the cross-source double-fire guard
 * (`dispatch-dedup.ts`, 300 ms) can swallow a fire that lands right after another
 * toggle. Idempotent once open.
 */
export async function openRail(main: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await railOpen(main)) return true
        await dispatchMenuCommand(main, 'askCmdr.toggle')
        return railOpen(main)
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}

/** Closes the rail via its header close button, so it can't bleed into a later surface. */
export async function closeRail(main: TauriPage): Promise<void> {
  if (!(await railOpen(main))) return
  await main.evaluate(`document.querySelector('${RAIL} .header-actions button:last-child')?.click()`)
  await expect.poll(() => railOpen(main), { timeout: waitBudget(3000) }).toBe(false)
}

/** The "Ask Cmdr is off" gate is up (rail open, chat not yet unlocked). */
function offGateShown(main: TauriPage): Promise<boolean> {
  return main.evaluate<boolean>(`document.querySelector('${RAIL} .ask-cmdr-gate[data-gate="off"]') !== null`)
}

/** The composer is present, meaning the rail is past its gates. */
function composerPresent(main: TauriPage): Promise<boolean> {
  return main.evaluate<boolean>(`document.querySelector('${RAIL} textarea') !== null`)
}

/** Completed fake assistant replies currently in the thread. */
function replyCount(main: TauriPage): Promise<number> {
  return main.evaluate<number>(
    `[...document.querySelectorAll('${RAIL} .msg')].filter(function(m){ return (m.textContent||'').includes('test assistant'); }).length`,
  )
}

/**
 * Turns Ask Cmdr on from the gate if it's showing, then waits for the composer.
 * The gate resolves asynchronously on open, so the composer isn't there on the
 * first tick even when the switch was already on: always poll.
 */
export async function unlockChat(main: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await composerPresent(main)) return true
        if (await offGateShown(main)) {
          await main.evaluate(`document.querySelector('${RAIL} .ask-cmdr-gate[data-gate="off"] button')?.click()`)
        }
        return composerPresent(main)
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  // The rail flips at once; the backend learns the switch a save and a push later, and the
  // chat surface's send would otherwise be refused as off inside that window.
  await waitForAskCmdrOnInBackend(main)
}

/** Types a message into the composer and sends it. */
export async function sendToRail(main: TauriPage, text: string): Promise<void> {
  await main.evaluate(`(function(){
    var ta = document.querySelector('${RAIL} textarea');
    if (!ta) throw new Error('no composer');
    ta.focus();
    ta.value = ${JSON.stringify(text)};
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  })()`)
}

/**
 * Captures the five Ask Cmdr rail states, in the order a user meets them.
 *
 * Each is a main-window panel, so all five share the main sink and follow the
 * usual rhythm: reset + label + enable BEFORE the state renders (the gate copy
 * and the empty state both resolve at mount), stage, capture.
 *
 * `skipped` takes the gate surface when the profile already has Ask Cmdr on,
 * which is what a re-run against a warm data dir looks like.
 */
export async function captureAskCmdrSurfaces(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  skipped: string[],
): Promise<void> {
  await ensureAppReady(main)
  await closeRail(main)

  // ── The "Ask Cmdr is off" gate ─────────────────────────────────────────────
  // Must run before anything turns Ask Cmdr on, and only shows on a profile that
  // never has. Not a failure when it's gone: a warm data dir passed it long ago.
  await captureCall(main, 'reset')
  await captureCall(main, 'setSurface', 'ask-cmdr-gate-off')
  await captureCall<boolean>(main, 'enable')
  await openRail(main)
  const gated = await expect
    .poll(async () => offGateShown(main), { timeout: waitBudget(3000) })
    .toBe(true)
    .then(() => true)
    .catch(() => false)
  if (gated) {
    await captureSurface('ask-cmdr-gate-off', report, failed, async () => {
      // The turn-on button is the last thing the gate renders; waiting on it means
      // the shot can't catch a half-built screen.
      await main.waitForSelector(`${RAIL} .ask-cmdr-gate[data-gate="off"] button`, 5000)
      return { page: main }
    })
  } else {
    skipped.push('ask-cmdr-gate-off')
    console.warn(
      `[i18n-capture] surface ask-cmdr-gate-off SKIPPED: this profile already has Ask Cmdr on, and the gate ` +
        `shows only before that. A capture run against a fresh data dir gets it.`,
    )
  }
  await captureCall(main, 'disable').catch(() => {})

  // ── Empty thread ───────────────────────────────────────────────────────────
  await captureSurface('ask-cmdr-empty', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-empty')
    await captureCall<boolean>(main, 'enable')
    await unlockChat(main)
    // A fresh chat, whatever the profile holds: opening the rail loads its latest thread,
    // and a lane shard reaches here after the Ask Cmdr specs have left threads behind.
    await main.evaluate(`document.querySelector('${RAIL} .header-actions [aria-label="New chat"]')?.click()`)
    await main.waitForSelector(`${RAIL} .empty .empty-title`, 5000)
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})

  // ── One exchange ───────────────────────────────────────────────────────────
  // The reply streams from the scripted fake LLM, so it's deterministic and needs
  // no provider. Waiting on the reply COUNT (not "a reply exists") keeps this
  // honest if a bootstrapped thread already showed one.
  await captureSurface('ask-cmdr-chat', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-chat')
    await captureCall<boolean>(main, 'enable')
    const before = await replyCount(main)
    await sendToRail(main, 'Which of these folders is the biggest?')
    await expect.poll(() => replyCount(main), { timeout: waitBudget(15000) }).toBeGreaterThan(before)
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})

  // ── A turn that ran tools ──────────────────────────────────────────────────
  // The fake's `[tools]` script (`chat/session.rs`, `rail_fake_turn`) narrates three rounds of
  // tool calls, including a refused one and two repeats, so its bubble shows tool lines with
  // their outcomes and repeat tags, group headers, and the turn's stats line. Both groups are
  // opened (the turn's end closed them) and one line is expanded, so its preview, truncation
  // note, and Copy render too.
  await captureSurface('ask-cmdr-tools', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-tools')
    await captureCall<boolean>(main, 'enable')
    await main.evaluate(`document.querySelector('${RAIL} .header-actions [aria-label="New chat"]')?.click()`)
    await sendToRail(main, '[tools] Which of my drives has the most room?')
    await main.waitForSelector(`${RAIL} .msg .turn-stats`, waitBudget(15000))
    await expect
      .poll(
        () =>
          main.evaluate<number>(`(function(){
            var closed = document.querySelectorAll('${RAIL} .group-toggle[aria-expanded="false"]');
            closed.forEach(function(toggle){ toggle.click(); });
            return document.querySelectorAll('${RAIL} .group-toggle[aria-expanded="false"]').length;
          })()`),
        { timeout: waitBudget(3000) },
      )
      .toBe(0)
    // The drives line: a list, so its preview has rows and a note.
    await main.evaluate(`document.querySelectorAll('${RAIL} .tool-line')[1]?.querySelector('button.expand')?.click()`)
    await main.waitForSelector(`${RAIL} .preview .copy`, waitBudget(5000))
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})

  // ── The threads panel ──────────────────────────────────────────────────────
  // Opened from the rail header, over the thread the exchange above created, so
  // the list has a real row rather than its own empty state.
  await captureSurface('ask-cmdr-sessions', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-sessions')
    await captureCall<boolean>(main, 'enable')
    await main.evaluate(`document.querySelector('${RAIL} .header-actions button')?.click()`)
    await main.waitForSelector(`${RAIL} .sessions`, 5000)
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})
  // Back to the thread: the panel stays open across closing the rail, and would cover every later
  // surface's thread.
  await main.evaluate(`document.querySelector('${RAIL} .sessions-header button')?.click()`)
  await expect
    .poll(() => main.evaluate<boolean>(`document.querySelector('${RAIL} .sessions') === null`), {
      timeout: waitBudget(3000),
    })
    .toBe(true)

  await closeRail(main).catch(() => {})
}
