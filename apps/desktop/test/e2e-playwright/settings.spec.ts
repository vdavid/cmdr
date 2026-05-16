/**
 * E2E tests for the settings page.
 *
 * In production the settings UI lives in its own window (label `settings`).
 * Each test opens it via the production trigger (`open-settings` Tauri event
 * → `openSettingsWindow()` → new `WebviewWindow`), then scopes a `TauriPage`
 * to the new window via `tauriPage.waitForWindow(w => w.label === 'settings')`.
 * The scoped page shares the plugin socket with the main page.
 */

import { test, expect } from './fixtures.js'
import { closeScopedWindow, openSettingsWindowViaProd, pollUntil } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

test.describe('Settings page', () => {
  let settings: TauriPage

  test.beforeEach(async ({ tauriPage }) => {
    settings = await openSettingsWindowViaProd(tauriPage as TauriPage)
    // 3 s: the settings window mounts in well under 1 s on a healthy machine.
    // The previous 15 s / 10 s budgets exceeded the suite's 8 s per-test ceiling
    // and just hid failures behind the outer timeout.
    await settings.waitForSelector('.settings-window', 3000)
    await settings.waitForSelector('.settings-sidebar', 3000)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, settings, 'settings')
  })

  test('renders the settings page', async () => {
    expect(await settings.isVisible('.settings-window')).toBe(true)
    expect(await settings.isVisible('.settings-layout')).toBe(true)
  })

  test('displays sidebar with sections', async () => {
    expect(await settings.isVisible('.settings-sidebar')).toBe(true)
    const sectionCount = await settings.count('.section-item')
    expect(sectionCount).toBeGreaterThan(0)
  })

  test('shows expected sections like Appearance and Keyboard shortcuts', async () => {
    const sectionTexts = await settings.allTextContents('.section-item')
    expect(sectionTexts.some((t) => t.includes('Appearance'))).toBe(true)
    expect(sectionTexts.some((t) => t.includes('Keyboard shortcuts'))).toBe(true)
  })

  test('has a working search input', async () => {
    await settings.waitForSelector('.search-input', 5000)
    await settings.fill('.search-input', 'theme')

    // Wait for the input value to be set
    await pollUntil(
      settings,
      async () => {
        const value = await settings.inputValue('.search-input')
        return value === 'theme'
      },
      3000,
    )

    const value = await settings.inputValue('.search-input')
    expect(value).toBe('theme')

    // Clear search
    await settings.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (input) {
                input.value = '';
                input.dispatchEvent(new Event('input', { bubbles: true }));
            }
        })()`)

    await pollUntil(
      settings,
      async () => {
        const val = await settings.inputValue('.search-input')
        return val === ''
      },
      3000,
    )
  })

  test('navigates between sections when clicking', async () => {
    await settings.waitForSelector('.settings-sidebar', 5000)

    // Wait for at least 2 section items
    await pollUntil(
      settings,
      async () => {
        const count = await settings.count('.section-item')
        return count >= 2
      },
      10000,
    )

    // Click second section
    await settings.evaluate(`(function() {
            var items = document.querySelectorAll('.section-item');
            if (items[1]) items[1].click();
        })()`)

    // Wait for section to become selected
    await pollUntil(
      settings,
      async () => {
        const cls = await settings.evaluate<string>(
          `document.querySelectorAll('.section-item')[1]?.getAttribute('class') || ''`,
        )
        return cls.includes('selected')
      },
      3000,
    )

    const classAttr = await settings.evaluate<string>(
      `document.querySelectorAll('.section-item')[1]?.getAttribute('class') || ''`,
    )
    expect(classAttr).toContain('selected')
  })

  test('search narrows the visible sidebar sections and clearing restores them', async () => {
    // Capture the baseline section count with no filter applied.
    await settings.waitForSelector('.section-item', 5000)
    const baselineCount = await settings.count('.section-item')
    expect(baselineCount).toBeGreaterThan(2)

    // Filter to a narrow query (only Appearance-tied options should match).
    await settings.fill('.search-input', 'accent')

    // Sidebar updates on a 200 ms debounce: wait for the count to drop.
    const narrowed = await pollUntil(
      settings,
      async () => {
        const count = await settings.count('.section-item')
        return count > 0 && count < baselineCount
      },
      3000,
    )
    expect(narrowed).toBe(true)

    // Clear button must reset both the input and the visible section list.
    await settings.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)

    const restored = await pollUntil(
      settings,
      async () => {
        const count = await settings.count('.section-item')
        const value = await settings.inputValue('.search-input')
        return count === baselineCount && value === ''
      },
      3000,
    )
    expect(restored).toBe(true)
  })

  test('search shows an empty sidebar for queries with no matches', async () => {
    await settings.waitForSelector('.section-item', 5000)
    await settings.fill('.search-input', 'zzzyyyxxxnomatch')

    // Sidebar updates on a 200 ms debounce: wait for all sections to vanish.
    const empty = await pollUntil(settings, async () => (await settings.count('.section-item')) === 0, 3000)
    expect(empty).toBe(true)

    // The clear button still shows up so the user can recover from a dead-end query.
    expect(await settings.isVisible('.search-clear')).toBe(true)

    // Reset state for the next test in the file.
    await settings.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)
    await pollUntil(settings, async () => (await settings.count('.section-item')) > 0, 3000)
  })

  test('Arrow Down in the search box moves section selection forward', async () => {
    // Reset any leftover search state from prior tests so the full sidebar is
    // rendered and the currently selected section is visible.
    await settings.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (!input) return;
            var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
            if (desc && desc.set) desc.set.call(input, '');
            else input.value = '';
            input.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await pollUntil(settings, async () => (await settings.count('.section-item')) > 2, 3000)

    await settings.waitForSelector('.section-item.selected', 5000)
    const startSelected = await settings.evaluate<string>(
      `document.querySelector('.section-item.selected')?.textContent?.trim() || ''`,
    )

    // Focus the search input then press Arrow Down: handler must forward to
    // the section list and advance the selection (no separate focus state).
    await settings.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            if (input) input.focus();
        })()`)

    await settings.evaluate(`(function() {
            var input = document.querySelector('.search-input');
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
        })()`)

    const advanced = await pollUntil(
      settings,
      async () => {
        const now = await settings.evaluate<string>(
          `document.querySelector('.section-item.selected')?.textContent?.trim() || ''`,
        )
        return now !== startSelected && now !== ''
      },
      3000,
    )
    expect(advanced).toBe(true)
  })
})

test.describe('Settings keyboard binding', () => {
  // The shared `closeScopedWindow` helper closes the settings window via
  // `plugin:window|close` from the main page, bypassing the keyboard handler
  // entirely. This block covers the actual Escape → getCurrentWindow().close()
  // binding in `routes/settings/+page.svelte`.

  test('Escape closes the settings window (production binding)', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const settings = await openSettingsWindowViaProd(main)
    await settings.waitForSelector('.settings-window', 3000)

    // Verify focus is actually inside the settings webview before pressing
    // Escape. Without this the keystroke can land on the main window (or
    // wherever focus drifted to during async onMount in the settings UI),
    // the settings window never receives it, and the test sits waiting for
    // a window-close that won't come. Two attempts max as cheap insurance.
    const tryEscape = async (): Promise<boolean> => {
      const focused = await pollUntil(
        settings,
        async () =>
          settings.evaluate<boolean>(`(function(){
            if (!document.hasFocus()) return false;
            var root = document.querySelector('.settings-window');
            return !!(root && document.activeElement && root.contains(document.activeElement));
          })()`),
        1000,
      )
      if (!focused) {
        // Re-focus inside the settings webview and let the caller retry.
        await settings.evaluate(`(function(){
          var root = document.querySelector('.settings-window');
          if (root && 'focus' in root) root.focus();
        })()`)
        return false
      }
      // Fire-and-forget: the dispatched Escape triggers getCurrentWindow().close()
      // synchronously inside the handler, so the window may die before pw_result
      // fires back. The poll on listWindows() below is the assertion.
      settings.keyboard.press('Escape').catch(() => {
        /* window died mid-script before pw_result; expected */
      })
      return true
    }

    if (!(await tryEscape())) {
      await tryEscape()
    }

    const gone = await pollUntil(
      main,
      async () => {
        const labels = (await main.listWindows()).map((w) => w.label)
        return !labels.includes('settings')
      },
      3000,
    )
    if (!gone) {
      throw new Error("Escape did not close settings window 'settings' within 3s")
    }
  })
})
