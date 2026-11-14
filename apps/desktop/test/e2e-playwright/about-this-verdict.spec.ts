/**
 * E2E for the "Tell me about this" card's AI take and "Continue in Ask Cmdr".
 *
 * The take comes from the E2E fake (`CMDR_E2E_ASK_CMDR_FAKE`, `e2e_fake::ABOUT_THIS_FAKE_VERDICT`), held mid-stream by
 * the harness's `CMDR_E2E_ABOUT_VERDICT_PACE` (a wait before each delta, a long hold before the last), so Stop lands on
 * a take that's still writing. The gate still reads `ai.provider`, so the take tests set it to `cloud` through MCP's
 * `set_setting` and put it back to `off` after each test (every other spec keeps it off). The fake answers before any
 * consent check, so nothing leaves the machine.
 *
 * "Continue in Ask Cmdr" shows only while Ask Cmdr is on: that test turns it on through the rail's own gate (MCP can't
 * set `askCmdr.enabled`) and back off through Settings, with `ai.provider` off so the rail opens on its chat.
 */

import fs from 'fs'
import path from 'path'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeRailIfOpen, ensureChatReady, openRailViaMenu, setAskCmdrEnabledInSettings } from './ask-cmdr-helpers.js'
import { pressFooterAction } from './about-this-helpers.js'
import { dismissOverlay, ensureAppReady, getFixtureRoot, moveCursorToFile, pressKey } from './helpers.js'
import { defaultKey } from './helpers/registry-keys.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const CARD = '[data-dialog-id="tell-me-about-this"]'
const VERDICT = `${CARD} section[aria-labelledby="about-verdict-label"]`
const DIR = 'about-this-verdict'
const FILE = 'notes.txt'
/** The fake's first and last deltas (`ABOUT_THIS_FAKE_VERDICT`). */
const FIRST_WORDS = 'This is a test verdict'
const LAST_WORDS = "and it can't tell what depends on this."

function fixtureDir(): string {
  return path.join(getFixtureRoot(), DIR)
}

async function setProvider(page: TauriPage, provider: 'off' | 'cloud'): Promise<void> {
  await ensureMcpClient(page)
  await mcpCall('set_setting', { id: 'ai.provider', value: provider })
}

async function openCardOnFixture(page: TauriPage): Promise<void> {
  await ensureAppReady(page)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mcp-nav-to-path',
      payload: { pane: 'left', path: ${JSON.stringify(fixtureDir())} }
  })`)
  expect(await moveCursorToFile(page, FILE)).toBe(true)
  await pressKey(page, defaultKey('file.tellMeAbout'))
  await page.waitForSelector(CARD, waitBudget(5000))
}

function verdictPhase(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`document.querySelector('${VERDICT}')?.getAttribute('data-verdict') ?? 'none'`)
}

function verdictText(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`(document.querySelector('${VERDICT}')?.textContent ?? '').replace(/\\s+/g, ' ')`)
}

function cardOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`!!document.querySelector('${CARD}')`)
}

/**
 * Opens the card with the provider on. The provider reaches the backend a push after `set_setting` answers, so a card
 * opened in between gets no take: close and reopen until it plans one.
 */
async function openCardWithATake(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await cardOpen(page)) await dismissOverlay(page)
        await openCardOnFixture(page)
        await expect.poll(() => cardOpen(page), { timeout: waitBudget(3000) }).toBe(true)
        return (await verdictPhase(page)) !== 'none'
      },
      { timeout: waitBudget(15000) },
    )
    .toBe(true)
}

test.describe('Tell me about this: the AI take', () => {
  test.beforeEach(async ({ tauriPage }) => {
    fs.rmSync(fixtureDir(), { recursive: true, force: true })
    fs.mkdirSync(fixtureDir(), { recursive: true })
    fs.writeFileSync(path.join(fixtureDir(), FILE), 'Groceries\nMilk\nBread\n')
    await setProvider(tauriPage as TauriPage, 'cloud')
  })
  test.afterEach(async ({ tauriPage }) => {
    if (await cardOpen(tauriPage as TauriPage)) await dismissOverlay(tauriPage)
    await setProvider(tauriPage as TauriPage, 'off')
    fs.rmSync(fixtureDir(), { recursive: true, force: true })
  })

  test('streams a labeled take right above the removal row, says what it was given first, and Stop keeps its text and the card', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    await openCardWithATake(page)
    await expect.poll(() => verdictText(page), { timeout: waitBudget(10000) }).toContain(FIRST_WORDS)
    expect(await verdictPhase(page)).toBe('streaming')
    const text = await verdictText(page)
    expect(text).toContain('AI wrote this')
    expect(text).toContain('From the facts above and its first lines.')
    // Its place: right above What removing it means.
    expect(
      await page.evaluate<boolean>(
        `!!document.querySelector('${CARD} [data-verdict-slot]')?.nextElementSibling?.querySelector('#about-section-removal')`,
      ),
    ).toBe(true)

    // Stop, while the fake holds its last delta.
    expect(
      await page.evaluate<boolean>(`(() => {
        const stop = [...document.querySelectorAll('${VERDICT} button')].find((b) => b.textContent.trim() === 'Stop')
        if (!stop) return false
        stop.click()
        return true
      })()`),
    ).toBe(true)
    await expect.poll(() => verdictPhase(page), { timeout: waitBudget(5000) }).toBe('stopped')
    const stopped = await verdictText(page)
    expect(stopped).toContain(FIRST_WORDS)
    expect(stopped).not.toContain(LAST_WORDS)
    expect(stopped).toContain('Stopped')
    expect(await cardOpen(page)).toBe(true)
  })

  test('a finished take comes back from earlier on a reopen, and Escape closes the card while one writes', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    await openCardWithATake(page)
    await expect.poll(() => verdictPhase(page), { timeout: waitBudget(15000) }).toBe('done')
    expect(await verdictText(page)).toContain(LAST_WORDS)
    await dismissOverlay(page)
    await expect.poll(() => cardOpen(page), { timeout: waitBudget(3000) }).toBe(false)

    // The same item, unchanged: no second call.
    await openCardOnFixture(page)
    await expect.poll(() => verdictPhase(page), { timeout: waitBudget(5000) }).toBe('fromEarlier')
    expect(await verdictText(page)).toContain(LAST_WORDS)
    await dismissOverlay(page)

    // Changed since: a new take, and Escape closes the card mid-stream.
    fs.appendFileSync(path.join(fixtureDir(), FILE), 'Eggs\n')
    await openCardOnFixture(page)
    await expect.poll(() => verdictText(page), { timeout: waitBudget(10000) }).toContain(FIRST_WORDS)
    expect(await verdictPhase(page)).toBe('streaming')
    await dismissOverlay(page)
    await expect.poll(() => cardOpen(page), { timeout: waitBudget(3000) }).toBe(false)
  })
})

test.describe('Tell me about this: with AI off', () => {
  test.beforeEach(() => {
    fs.rmSync(fixtureDir(), { recursive: true, force: true })
    fs.mkdirSync(fixtureDir(), { recursive: true })
    fs.writeFileSync(path.join(fixtureDir(), FILE), 'Groceries\nMilk\nBread\n')
  })
  test.afterEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    if (await cardOpen(page)) await dismissOverlay(page)
    await closeRailIfOpen(page)
    fs.rmSync(fixtureDir(), { recursive: true, force: true })
  })

  test('draws no take, only a footer hint', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await setProvider(page, 'off')
    await openCardOnFixture(page)
    await expect
      .poll(() => page.evaluate<boolean>(`!!document.querySelector('${CARD} [data-settled="true"]')`), {
        timeout: waitBudget(5000),
      })
      .toBe(true)
    expect(await page.count(`${CARD} [data-verdict-slot]`)).toBe(0)
    expect(await page.textContent(`${CARD} .ai-off`)).toContain('AI is off')
  })

  test('"Continue in Ask Cmdr" opens a new chat with the item attached and the fixed question, unsent', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    await setProvider(page, 'off')
    // Ask Cmdr on, through the rail's own gate.
    await openRailViaMenu(page)
    await ensureChatReady(page)
    await closeRailIfOpen(page)
    try {
      await openCardOnFixture(page)
      await expect
        .poll(() => page.evaluate<boolean>(`!!document.querySelector('${CARD} [data-about-actions] [data-action]')`), {
          timeout: waitBudget(5000),
        })
        .toBe(true)
      await pressFooterAction(page, ['continueInAskCmdr'])

      // The card closed first; the rail holds a fresh chat with the item and the draft, and nothing was sent.
      await expect.poll(() => cardOpen(page), { timeout: waitBudget(3000) }).toBe(false)
      await expect
        .poll(() => page.evaluate<string>(`document.querySelector('.ask-cmdr-rail textarea')?.value ?? ''`), {
          timeout: waitBudget(5000),
        })
        .toBe('Tell me more about this.')
      expect(await page.textContent('.ask-cmdr-rail')).toContain(FILE)
      expect(await page.count('.ask-cmdr-rail .msg')).toBe(0)
      expect(
        await page.evaluate<boolean>(`document.activeElement === document.querySelector('.ask-cmdr-rail textarea')`),
      ).toBe(true)
    } finally {
      await closeRailIfOpen(page)
      await setAskCmdrEnabledInSettings(page, false)
    }
  })
})
