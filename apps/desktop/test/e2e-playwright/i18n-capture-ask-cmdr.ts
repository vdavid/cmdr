/**
 * Ask Cmdr surface captures for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * Ask Cmdr is the largest single uncoupled area in the catalog, and none of it is
 * reachable from a dialog trigger: the rail is a panel inside the main window
 * whose content depends on consent, on a provider being willing to answer, and on
 * there being threads to list. So this module walks the rail the way
 * `ask-cmdr.spec.ts` does, capturing four states in the order the user meets them:
 *
 *  1. `ask-cmdr-consent`: the opt-in gate a fresh profile opens to.
 *  2. `ask-cmdr-empty`: consented, no messages yet.
 *  3. `ask-cmdr-chat`: one exchange, so the message chrome, the thinking line, and
 *     the cost footer render.
 *  4. `ask-cmdr-sessions`: the threads panel over a thread that exists.
 *
 * The replies come from the scripted fake LLM, not a real provider: the capture
 * launch sets `CMDR_E2E_ASK_CMDR_FAKE=1`, which is the single source of truth for
 * both `resolve_agent_llm` (which answers) and the composer's provider gate (which
 * allows the send), so the two can't disagree. Without it the composer refuses to
 * send and the chat surface would photograph an empty thread.
 *
 * The consent surface is BEST-EFFORT: consent lives in `main.db` and persists for
 * the life of the data dir, so a second capture run against a warm dir opens
 * straight to the composer. It's captured when the gate shows and recorded as a
 * documented skip when it doesn't, rather than faking a screen the user has
 * already passed.
 */

import { expect } from './fixtures.js'
import { ensureAppReady, dispatchMenuCommand } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface } from './i18n-capture-helpers.js'

const RAIL = '.ask-cmdr-rail'

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
async function openRail(main: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await railOpen(main)) return true
        await dispatchMenuCommand(main, 'askCmdr.toggle')
        return railOpen(main)
      },
      { timeout: 5000 },
    )
    .toBe(true)
}

/** Closes the rail via its header close button, so it can't bleed into a later surface. */
async function closeRail(main: TauriPage): Promise<void> {
  if (!(await railOpen(main))) return
  await main.evaluate(`document.querySelector('${RAIL} .header-actions button:last-child')?.click()`)
  await expect.poll(() => railOpen(main), { timeout: 3000 }).toBe(false)
}

/** The opt-in consent screen is up (rail open, chat not yet unlocked). */
function consentShown(main: TauriPage): Promise<boolean> {
  return main.evaluate<boolean>(`document.querySelector('${RAIL} .consent') !== null`)
}

/** The composer is present, meaning the rail is unlocked past consent. */
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
 * Accepts the consent opt-in if the gate is showing, then waits for the composer.
 * Consent resolves asynchronously on open, so the composer isn't there on the
 * first tick even when consent was already granted: always poll.
 */
async function unlockChat(main: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await composerPresent(main)) return true
        if (await consentShown(main)) {
          await main.evaluate(`document.querySelector('${RAIL} .consent .consent-accept')?.click()`)
        }
        return composerPresent(main)
      },
      { timeout: 5000 },
    )
    .toBe(true)
}

/**
 * Captures the four Ask Cmdr rail states, in the order a user meets them.
 *
 * Each is a main-window panel, so all four share the main sink and follow the
 * usual rhythm: reset + label + enable BEFORE the state renders (the consent copy
 * and the empty state both resolve at mount), stage, capture.
 *
 * `skipped` takes the consent surface when the profile is already consented,
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

  // ── The consent gate ───────────────────────────────────────────────────────
  // Must run before anything accepts it, and only exists on a profile that never
  // has. Not a failure when it's gone: it's a one-time screen.
  await captureCall(main, 'reset')
  await captureCall(main, 'setSurface', 'ask-cmdr-consent')
  await captureCall<boolean>(main, 'enable')
  await openRail(main)
  const needsConsent = await expect
    .poll(async () => consentShown(main), { timeout: 3000 })
    .toBe(true)
    .then(() => true)
    .catch(() => false)
  if (needsConsent) {
    await captureSurface('ask-cmdr-consent', report, failed, async () => {
      // The accept button is the last thing the gate renders; waiting on it means
      // the shot can't catch a half-built screen.
      await main.waitForSelector(`${RAIL} .consent .consent-accept`, 5000)
      return { page: main }
    })
  } else {
    skipped.push('ask-cmdr-consent')
    console.warn(
      `[i18n-capture] surface ask-cmdr-consent SKIPPED: this profile already consented, and the gate is a ` +
        `one-time screen. A capture run against a fresh data dir gets it.`,
    )
  }
  await captureCall(main, 'disable').catch(() => {})

  // ── Empty thread ───────────────────────────────────────────────────────────
  await captureSurface('ask-cmdr-empty', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'ask-cmdr-empty')
    await captureCall<boolean>(main, 'enable')
    await unlockChat(main)
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
    await main.evaluate(`(function(){
      var ta = document.querySelector('${RAIL} textarea');
      if (!ta) throw new Error('no composer');
      ta.focus();
      ta.value = 'Which of these folders is the biggest?';
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)
    await expect.poll(() => replyCount(main), { timeout: 15000 }).toBeGreaterThan(before)
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

  await closeRail(main).catch(() => {})
}
