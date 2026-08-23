/**
 * E2E for the proactive half of Ask Cmdr: the agent noticing something and opening a thread
 * about it, with nobody having typed anything.
 *
 * This is the first test that runs the whole lane together — the rollup channel, the writer
 * thread that owns the inbox, the wake thread, and the chat runtime. It drives it through
 * `force_agent_wake` (`playwright-e2e` only), which stages one folder's activity on the same
 * channel the indexer's tap uses and then tells the loop to act now instead of sitting out a
 * cadence that runs up to half an hour.
 *
 * The turn answers from the deterministic fake (`CMDR_E2E_ASK_CMDR_FAKE=1`), which speaks a
 * DIFFERENT sentence for a wake than for the rail. That difference is what tells a thread the
 * agent opened apart from one the user did, and it's why `ask-cmdr.spec.ts`'s "test assistant"
 * matching keeps working untouched.
 *
 * ⚠️ Consent gates everything here: without it the pipeline stores nothing and runs nothing.
 * The rail's opt-in screen is the only way to grant it, so the test opens the rail first.
 */

import { test, expect } from './fixtures.js'
import { dismissAllToasts, dispatchMenuCommand, ensureAppReady, forceAgentWake } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** What the wake's scripted fake says, distinct from the rail's reply on purpose. */
const WAKE_REPLY = 'I had a look at what changed.'

/** The rail is open once its root element is in the DOM. */
function railOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail') !== null`)
}

/** Everything the open rail currently shows, for a contains-check. */
function railText(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`document.querySelector('.ask-cmdr-rail')?.textContent || ''`)
}

/** The composer is present, which means the rail is unlocked past consent. */
function composerPresent(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail textarea') !== null`)
}

/** The opt-in consent screen is showing. */
function consentShown(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail .consent') !== null`)
}

/** The thread titles the sessions panel is currently listing. */
function sessionTitles(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `[...document.querySelectorAll('.ask-cmdr-rail .sessions .row-title')].map(r => r.textContent).join('|')`,
  )
}

/** Opens the sessions panel, which reloads the list from the top every time (`openSessions`),
 * so calling it again is how a poll sees a thread that appeared meanwhile. */
async function reloadSessions(page: TauriPage): Promise<void> {
  await page.evaluate(`document.querySelector('.ask-cmdr-rail [aria-label="Chats"]')?.click()`)
}

/**
 * Arms a DOM watch for the status corner's wake indicator, then answers once it has gone busy and
 * back to idle: a full wake, start to finish.
 *
 * ⚠️ A `MutationObserver` rather than a poll, and armed BEFORE the wake is forced. The fake answers
 * in milliseconds, so a poll would routinely sample after the indicator had already come and gone
 * and would report "idle" without having waited for anything. The observer cannot miss the
 * transition, and the `busy` flag is what makes the wait non-vacuous: an indicator that never
 * appeared leaves `idle` false and the poll below times out rather than passing early.
 */
async function watchForWake(page: TauriPage): Promise<void> {
  await page.evaluate(`(() => {
    const seen = { busy: false, idle: false }
    window.__wakeSeen = seen
    const check = () => {
      if (document.querySelector('.status-corner .wake-indicator .thinking') !== null) seen.busy = true
      else if (seen.busy) seen.idle = true
    }
    check()
    new MutationObserver(check).observe(document.body, { subtree: true, childList: true })
  })()`)
}

/**
 * Arms a DOM watch for the staged-proposal toast, then remembers what it said.
 *
 * ⚠️ A `MutationObserver` rather than a poll, for a second reason on top of `watchForWake`'s:
 * this toast AUTO-DISMISSES after four seconds, on purpose (the proposals wait in the
 * suggestions badge either way). A poll could sample after it had come and gone and report a
 * missing toast that was in fact perfectly raised.
 */
async function watchForStagedToast(page: TauriPage): Promise<void> {
  await page.evaluate(`(() => {
    window.__wakeToast = null
    const check = () => {
      const body = document.querySelector('.toast-container .toast-body')
      if (body !== null && window.__wakeToast === null) window.__wakeToast = body.textContent || ''
    }
    check()
    new MutationObserver(check).observe(document.body, { subtree: true, childList: true })
  })()`)
}

/** What the staged-proposal toast said, once it has appeared. */
function stagedToastText(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`window.__wakeToast || ''`)
}

/** Waits for the wake armed by `watchForWake` to have run and finished. */
async function awaitWakeFinished(page: TauriPage): Promise<void> {
  await expect.poll(() => page.evaluate<boolean>(`window.__wakeSeen?.idle === true`), { timeout: 20000 }).toBe(true)
}

/** Opens the rail via the View-menu toggle, re-dispatching inside the poll: the cross-source
 * double-fire guard (dispatch-dedup.ts, 300ms) can drop a fire that lands right after another
 * test's toggle. Idempotent once open. */
async function openRail(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await railOpen(page)) return true
        await dispatchMenuCommand(page, 'askCmdr.toggle')
        return railOpen(page)
      },
      { timeout: 5000 },
    )
    .toBe(true)
}

/** Closes the rail via its header close button, if open. */
async function closeRailIfOpen(page: TauriPage): Promise<void> {
  if (!(await railOpen(page))) return
  await page.evaluate(`document.querySelector('.ask-cmdr-rail .header-actions button:last-child')?.click()`)
  await expect.poll(() => railOpen(page), { timeout: 3000 }).toBe(false)
}

/** Grants consent if the gate is showing (it persists in `main.db` for the run), then waits
 * for the composer. Consent resolves asynchronously on open, so the composer isn't there on
 * the first tick even when already granted — always wait. */
async function ensureConsented(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await composerPresent(page)) return true
        if (await consentShown(page)) {
          await page.evaluate(`document.querySelector('.ask-cmdr-rail .consent .consent-accept')?.click()`)
        }
        return composerPresent(page)
      },
      { timeout: 5000 },
    )
    .toBe(true)
}

test.describe('Ask Cmdr wakes on its own', () => {
  test.describe.configure({ timeout: 40000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test('opens a thread about a folder that changed, without being asked', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // A per-run nonce, so the thread this test looks at can't be one an earlier run left
    // behind. The wake names its thread after the folder's last segment.
    const folderName = `wake-check-${String(Date.now())}`

    await openRail(page)
    await ensureConsented(page)

    await forceAgentWake(page, `/Users/e2e/${folderName}`)

    // The wake opens its thread before the turn runs, so the row shows up first.
    await expect
      .poll(
        async () => {
          await reloadSessions(page)
          return sessionTitles(page)
        },
        { timeout: 20000 },
      )
      .toContain(folderName)

    // Open it: the first message is what the agent noticed, and the reply is the wake fake's
    // own sentence.
    await page.evaluate(
      `[...document.querySelectorAll('.ask-cmdr-rail .sessions .row')]
        .find(r => (r.textContent || '').includes(${JSON.stringify(folderName)}))?.click()`,
    )
    await expect.poll(() => railText(page), { timeout: 15000 }).toContain(WAKE_REPLY)

    // ⚠️ The digest opens COLLAPSED and says nothing about which folder until it is expanded:
    // a thread opens on what the agent SAID, not on the tally that prompted it. And every word
    // of it is the catalog's — the backend sends counts and paths, never a sentence — which is
    // what this assertion is really pinning.
    const collapsed = await railText(page)
    expect(collapsed).toContain('What changed in 1 folder')
    expect(collapsed).not.toContain(folderName)

    await page.evaluate(`document.querySelector('.ask-cmdr-rail .wake-digest .digest-toggle')?.click()`)
    await expect.poll(() => railText(page), { timeout: 5000 }).toContain(folderName)
    const text = await railText(page)
    expect(text).toContain('5 new items')
    // ⚠️ The rail's own scripted reply must not appear: one shared script would make a wake
    // thread indistinguishable from a chat the user started, and would tie the two suites
    // together so that changing either one's copy broke the other.
    expect(text).not.toContain('test assistant')
  })

  test('leaves no thread behind when it has nothing to suggest', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // Two nonces: the quiet wake's folder, which must never reach the session list, and the
    // control's, which must. The control is what proves the lane actually ran — without it,
    // "no thread appeared" would also pass with the whole wake loop dead.
    const quietFolder = `wake-quiet-${String(Date.now())}`
    const loudFolder = `wake-loud-${String(Date.now())}`

    await openRail(page)
    await ensureConsented(page)

    await watchForWake(page)
    await forceAgentWake(page, `/Users/e2e/${quietFolder}`, 'quiet')

    // ⚠️ Absence can only be asserted at the END. A wake opens its thread BEFORE the turn runs
    // and deletes it after, so a poll mid-flight would legitimately see the row. Waiting for the
    // quiet wake to FINISH is also what keeps the control below out of it: any wake drains the
    // whole inbox, so a rollup landing before the first prepare would merge into the same one.
    await awaitWakeFinished(page)

    await forceAgentWake(page, `/Users/e2e/${loudFolder}`, 'reply')
    await expect
      .poll(
        async () => {
          await reloadSessions(page)
          return sessionTitles(page)
        },
        { timeout: 20000 },
      )
      .toContain(loudFolder)

    // The control landed, so the loop is alive and has been through both wakes. The quiet one
    // still left nothing.
    expect(await sessionTitles(page)).not.toContain(quietFolder)
  })

  test('says so when it stages something, and marks the thread it reasoned in', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const folderName = `wake-staged-${String(Date.now())}`

    await openRail(page)
    await ensureConsented(page)

    // Both watches armed BEFORE the wake: the toast dismisses itself after four seconds and
    // the whole turn takes milliseconds against the fake.
    await watchForStagedToast(page)
    await watchForWake(page)
    await forceAgentWake(page, `/Users/e2e/${folderName}`, 'propose')
    await awaitWakeFinished(page)

    // The one time the proactive agent interrupts: it proposed something nobody asked for, so
    // it says so, and it offers both ways in.
    await expect.poll(() => stagedToastText(page), { timeout: 20000 }).toContain('suggestion')
    const toast = await stagedToastText(page)
    expect(toast).toContain('Review')
    expect(toast).toContain('See why')

    // ⚠️ Put the fake back before asserting anything else: the script sticks, and a later spec
    // forcing a wake would otherwise stage another group.
    await forceAgentWake(page, `/Users/e2e/${folderName}-done`, 'reply')

    // And the thread it reasoned in wears the glyph, so it is not mistaken for one the user
    // started and forgot.
    await expect
      .poll(
        async () => {
          await reloadSessions(page)
          return sessionTitles(page)
        },
        { timeout: 20000 },
      )
      .toContain(folderName)
    const marked = await page.evaluate<boolean>(
      `[...document.querySelectorAll('.ask-cmdr-rail .sessions .conversation')]
        .filter(row => (row.textContent || '').includes(${JSON.stringify(folderName)}))
        .every(row => row.querySelector('.started-by-agent') !== null)`,
    )
    expect(marked).toBe(true)

    // The toast never auto-dismissed inside the test's own timing, so clear it: a toast left
    // standing is a UI artifact the fixture fails the run over, and rightly.
    await dismissAllToasts(page)
  })
})
