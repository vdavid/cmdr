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

  test('search narrows the visible sidebar sections and clearing restores them', async ({ tauriPage }) => {
    // Capture the baseline section count with no filter applied.
    await tauriPage.waitForSelector('.section-item', 5000)
    const baselineCount = await tauriPage.count('.section-item')
    expect(baselineCount).toBeGreaterThan(2)

    // Filter to a narrow query (only Appearance-tied options should match).
    await tauriPage.fill('.search-input', 'accent')

    // Sidebar updates on a 200 ms debounce — wait for the count to drop.
    const narrowed = await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.count('.section-item')
        return count > 0 && count < baselineCount
      },
      3000,
    )
    expect(narrowed).toBe(true)

    // Clear button must reset both the input and the visible section list.
    await tauriPage.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)

    const restored = await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.count('.section-item')
        const value = await tauriPage.inputValue('.search-input')
        return count === baselineCount && value === ''
      },
      3000,
    )
    expect(restored).toBe(true)
  })

  test('search shows an empty sidebar for queries with no matches', async ({ tauriPage }) => {
    await tauriPage.waitForSelector('.section-item', 5000)
    await tauriPage.fill('.search-input', 'zzzyyyxxxnomatch')

    // Sidebar updates on a 200 ms debounce — wait for all sections to vanish.
    const empty = await pollUntil(tauriPage, async () => (await tauriPage.count('.section-item')) === 0, 3000)
    expect(empty).toBe(true)

    // The clear button still shows up so the user can recover from a dead-end query.
    expect(await tauriPage.isVisible('.search-clear')).toBe(true)

    // Reset state for the next test in the file.
    await tauriPage.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)
    await pollUntil(tauriPage, async () => (await tauriPage.count('.section-item')) > 0, 3000)
  })

  test('Arrow Down in the search box moves section selection forward', async ({ tauriPage }) => {
    // Reset any leftover search state from prior tests so the full sidebar is
    // rendered and the currently selected section is visible.
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (!input) return;
            var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
            if (desc && desc.set) desc.set.call(input, '');
            else input.value = '';
            input.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await pollUntil(tauriPage, async () => (await tauriPage.count('.section-item')) > 2, 3000)

    await tauriPage.waitForSelector('.section-item.selected', 5000)
    const startSelected = await tauriPage.evaluate<string>(
      `document.querySelector('.section-item.selected')?.textContent?.trim() || ''`,
    )

    // Focus the search input then press Arrow Down — handler must forward to
    // the section list and advance the selection (no separate focus state).
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (input) input.focus();
        })()`)

    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
        })()`)

    const advanced = await pollUntil(
      tauriPage,
      async () => {
        const now = await tauriPage.evaluate<string>(
          `document.querySelector('.section-item.selected')?.textContent?.trim() || ''`,
        )
        return now !== startSelected && now !== ''
      },
      3000,
    )
    expect(advanced).toBe(true)
  })
})
