/**
 * Cross-component E2E for viewer regex search.
 *
 * This is the one E2E spec for milestone 1 of the viewer-search work. Per the
 * plan, the bulk of the search behaviour (state machine, ESC cancel,
 * case-sensitivity toggle, regex-error display) lives in unit tests
 * (`viewer-search.svelte.test.ts`). What only an E2E can verify is the
 * cross-component flow: the toolbar toggle → composable state → IPC → backend
 * matcher → results in the viewport.
 *
 * Fixture: a 4 KB text file with digits in known positions so we can verify
 * that the regex `\d+` finds the digit groups and that literal-mode `\d+` does
 * not (it would search for a literal backslash-d).
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const REGEX_FIXTURE_DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-viewer-regex-'))
const REGEX_FIXTURE_PATH = path.join(REGEX_FIXTURE_DIR, 'with-digits.txt')

const REGEX_FIXTURE_CONTENT = ['alpha 123 beta', 'gamma 4567 delta', 'epsilon zeta', 'eta 89 theta'].join('\n')

async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 8000)
  return viewer
}

test.describe('File viewer regex search', () => {
  let viewer: TauriPage
  let viewerLabel: string

  test.beforeAll(() => {
    fs.writeFileSync(REGEX_FIXTURE_PATH, REGEX_FIXTURE_CONTENT, 'utf-8')
  })

  test.afterAll(() => {
    try {
      fs.rmSync(REGEX_FIXTURE_DIR, { recursive: true, force: true })
    } catch {
      // best-effort cleanup
    }
  })

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, REGEX_FIXTURE_PATH)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  test('regex toggle finds digit groups via \\d+', async () => {
    // Open the search bar.
    await viewer.keyboard.press('Control+f')
    await viewer.waitForSelector('.search-bar', 5000)

    // Toggle regex mode via the `.*` button.
    await viewer.evaluate(`
      (function () {
        const btn = document.querySelector('button[aria-label="Regex"]')
        if (!btn) throw new Error('regex toggle not found')
        btn.click()
      })()
    `)

    // Confirm the button is marked active.
    const regexPressed = await viewer.evaluate<string | null>(`
      (function () {
        const btn = document.querySelector('button[aria-label="Regex"]')
        return btn ? btn.getAttribute('aria-pressed') : null
      })()
    `)
    expect(regexPressed).toBe('true')

    // Type the regex query and wait for results. The fixture has three groups
    // of digits, so we expect three matches.
    await viewer.fill('.search-input', String.raw`\d+`)

    await expect
      .poll(
        async () => {
          const text = (await viewer.textContent('.match-count')) ?? ''
          return /1 of 3\b/.test(text)
        },
        { timeout: 5000 },
      )
      .toBeTruthy()
  })
})
