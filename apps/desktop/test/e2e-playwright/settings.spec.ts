/**
 * E2E tests for the settings page.
 *
 * Uses SvelteKit's link-click interception for client-side navigation.
 * In Tauri, browser.url() doesn't work — creating a temporary `<a>` element
 * and clicking it triggers SvelteKit's client-side navigation.
 */

import { test, expect } from './fixtures.js'
import { navigateToRoute, pollUntil } from './helpers.js'

test.describe('Settings page', () => {
  test.beforeEach(async ({ tauriPage }) => {
    // Navigate to settings if not already there
    const onSettings = await tauriPage.isVisible('.settings-window')
    if (!onSettings) {
      await tauriPage.waitForSelector('.dual-pane-explorer', 15000)
      await navigateToRoute(tauriPage, '/settings')
      await tauriPage.waitForSelector('.settings-window', 15000)
      await tauriPage.waitForSelector('.settings-sidebar', 10000)
    }
  })

  test('renders the settings page', async ({ tauriPage }) => {
    expect(await tauriPage.isVisible('.settings-window')).toBe(true)
    expect(await tauriPage.isVisible('.settings-layout')).toBe(true)
  })

  test('displays sidebar with sections', async ({ tauriPage }) => {
    expect(await tauriPage.isVisible('.settings-sidebar')).toBe(true)
    const sectionCount = await tauriPage.count('.section-item')
    expect(sectionCount).toBeGreaterThan(0)
  })

  test('shows expected sections like Appearance and Keyboard shortcuts', async ({ tauriPage }) => {
    const sectionTexts = await tauriPage.allTextContents('.section-item')
    expect(sectionTexts.some((t) => t.includes('Appearance'))).toBe(true)
    expect(sectionTexts.some((t) => t.includes('Keyboard shortcuts'))).toBe(true)
  })

  test('has a working search input', async ({ tauriPage }) => {
    await tauriPage.waitForSelector('.search-input', 5000)
    await tauriPage.fill('.search-input', 'theme')

    // Wait for the input value to be set
    await pollUntil(
      tauriPage,
      async () => {
        const value = await tauriPage.inputValue('.search-input')
        return value === 'theme'
      },
      3000,
    )

    const value = await tauriPage.inputValue('.search-input')
    expect(value).toBe('theme')

    // Clear search
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (input) {
                input.value = '';
                input.dispatchEvent(new Event('input', { bubbles: true }));
            }
        })()`)

    await pollUntil(
      tauriPage,
      async () => {
        const val = await tauriPage.inputValue('.search-input')
        return val === ''
      },
      3000,
    )
  })

  test('navigates between sections when clicking', async ({ tauriPage }) => {
    await tauriPage.waitForSelector('.settings-sidebar', 5000)

    // Wait for at least 2 section items
    await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.count('.section-item')
        return count >= 2
      },
      10000,
    )

    // Click second section
    await tauriPage.evaluate(`(function() {
            var items = document.querySelectorAll('.section-item');
            if (items[1]) items[1].click();
        })()`)

    // Wait for section to become selected
    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.evaluate<string>(
          `document.querySelectorAll('.section-item')[1]?.getAttribute('class') || ''`,
        )
        return cls.includes('selected')
      },
      3000,
    )

    const classAttr = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.section-item')[1]?.getAttribute('class') || ''`,
    )
    expect(classAttr).toContain('selected')
  })
})
