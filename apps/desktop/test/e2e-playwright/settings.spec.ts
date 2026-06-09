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

/**
 * Clicks a sidebar `.section-item` by exact (trimmed) text. Returns false if no
 * matching item exists; the caller can then fail the test with a useful message.
 */
function clickSectionByTextJs(name: string): string {
  return `(function() {
    var items = document.querySelectorAll('.section-item');
    var target = ${JSON.stringify(name)};
    for (var i = 0; i < items.length; i++) {
      if ((items[i].textContent || '').trim() === target) {
        items[i].click();
        return true;
      }
    }
    return false;
  })()`
}

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

  test('lists top-level sections in the expected order', async () => {
    // Locks down the full sidebar shape. If you intentionally add/remove/reorder a section,
    // update both this list and `TOP_LEVEL_ORDER` in `SettingsSidebar.svelte`.
    const expectedOrder = [
      'Appearance',
      'Colors and formats',
      'Zoom and density',
      'File and folder sizes',
      'Listing',
      'Behavior',
      'File operations',
      'File system watching',
      'Search',
      'AI',
      'File systems',
      'SMB/Network shares',
      'MTP (Android/Kindle/cameras)',
      'Git',
      'Viewer',
      'Keyboard shortcuts',
      'Developer',
      'MCP server',
      'Logging',
      'Updates & privacy',
      'License',
      'Advanced',
    ]

    const sectionTexts = await settings.allTextContents('.section-item')
    const trimmed = sectionTexts.map((t) => t.trim())
    expect(trimmed).toEqual(expectedOrder)
  })

  test('clicking a subsection routes to the matching section component', async () => {
    // Default boot lands on Appearance > Colors and formats; navigate elsewhere so we
    // can prove the click handler swapped the content. "File operations" is a small
    // subsection (one setting) so this stays fast.
    const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('File operations'))
    expect(clicked).toBe(true)

    // The content area renders a wrapper `<section data-section-id="...">` per matched
    // path. Wait for the new wrapper to appear, then read its header + visible labels.
    await settings.waitForSelector('[data-section-id="behavior-file-operations"]', 3000)
    const probe = await settings.evaluate<{ title: string; labels: string[] }>(
      `(function() {
        var wrapper = document.querySelector('[data-section-id="behavior-file-operations"]');
        if (!wrapper) return { title: '', labels: [] };
        var title = (wrapper.querySelector('.section-title')?.textContent || '').trim();
        var labels = Array.from(wrapper.querySelectorAll('.setting-label')).map(function(el) {
          return (el.textContent || '').trim();
        });
        return { title: title, labels: labels };
      })()`,
    )
    expect(probe.title).toBe('File operations')
    expect(probe.labels).toContain('Allow file extension changes')

    // No leftover Appearance content under the wrapper (would mean both rendered together).
    const otherSectionPresent = await settings.evaluate<boolean>(
      `!!document.querySelector('[data-section-id="appearance-colors-and-formats"]')`,
    )
    expect(otherSectionPresent).toBe(false)
  })

  test('selecting a top-level section with subsections renders summary cards', async () => {
    // Appearance has 4 navigable subsections; SectionSummary should surface each as a
    // `.subsection-card` with a name + description. This catches a regression in either
    // the summary trigger list in SettingsContent (`sectionsWithSubsections`) or the
    // SectionSummary component itself.
    const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('Appearance'))
    expect(clicked).toBe(true)

    await settings.waitForSelector('.subsection-card', 3000)
    const probe = await settings.evaluate<{ names: string[]; firstDescription: string }>(
      `(function() {
        var cards = Array.from(document.querySelectorAll('.subsection-card'));
        var names = cards.map(function(c) { return (c.querySelector('.subsection-name')?.textContent || '').trim(); });
        var firstDescription = cards.length
          ? (cards[0].querySelector('.subsection-description')?.textContent || '').trim()
          : '';
        return { names: names, firstDescription: firstDescription };
      })()`,
    )
    expect(probe.names).toEqual(['Colors and formats', 'Zoom and density', 'File and folder sizes', 'Listing'])
    // Sanity-check that the SectionSummary description lookup table is wired up (not the
    // fallback string). The fallback for "Colors and formats" would be
    // "Configure colors and formats settings." — assert we got the curated copy instead.
    expect(probe.firstDescription).toBe('Theme, app color, date and size coloring, and date/time format.')
  })

  test('Colors and formats hosts the theme.mode row (regression for the old Themes section)', async () => {
    // `theme.mode` used to live in a standalone Themes top-level section. After the
    // reorg it folds into Appearance > Colors and formats. If the registry path or the
    // section component drifts apart, this row vanishes — catch that immediately.
    const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('Colors and formats'))
    expect(clicked).toBe(true)

    await settings.waitForSelector('[data-section-id="appearance-colors-and-formats"]', 3000)
    const labels = await settings.evaluate<string[]>(
      `Array.from(document.querySelectorAll('[data-section-id="appearance-colors-and-formats"] .setting-label'))
        .map(function(el) { return (el.textContent || '').trim(); })`,
    )
    // Theme mode at the top, plus a couple of canaries to confirm the rest of the
    // subsection didn't lose rows in the rewrite.
    expect(labels).toContain('Theme mode')
    expect(labels).toContain('App color')
    expect(labels).toContain('Striped rows')
  })

  test('toggle-group items pick up their CSS (regression: Svelte 5 scoping through Ark)', async () => {
    // `ToggleGroup` with `semantics="toggles"` wraps Ark UI's `ToggleGroup.Root`. Svelte 5
    // doesn't propagate the component's scoping hash through Ark's `class` forwarding, so
    // `.tg-root.svelte-<hash> .tg-item { ... }` rules whiff against Ark-rendered DOM. The
    // items then render unstyled (no padding, no separating border, no active background),
    // collapsing visually into a single run-on label like "SmartContentOn disk".
    //
    // File and folder sizes is the densest user of toggle groups in Settings (3 rows).
    const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('File and folder sizes'))
    expect(clicked).toBe(true)
    await settings.waitForSelector('[data-section-id="appearance-file-and-folder-sizes"]', 3000)
    await settings.waitForSelector('[data-scope="toggle-group"][data-part="item"]', 3000)

    // Probe computed style on a non-last item: `.tg-root .tg-item` sets padding for every
    // item and `border-right` for every item except the last. Both reading "0px" means the
    // rules never matched. Reading anything non-zero proves scoping landed.
    const styles = await settings.evaluate<{ padding: string; borderRightWidth: string }>(
      `(function() {
        var item = document.querySelector('[data-scope="toggle-group"][data-part="item"]');
        if (!item) return { padding: '', borderRightWidth: '' };
        var cs = getComputedStyle(item);
        return { padding: cs.padding, borderRightWidth: cs.borderRightWidth };
      })()`,
    )
    expect(styles.padding).not.toBe('0px')
    expect(styles.borderRightWidth).not.toBe('0px')
  })

  test('Advanced section renders auto-generated rows for showInAdvanced entries', async () => {
    // The Advanced page is registry-driven: every `showInAdvanced: true` entry becomes a
    // `.advanced-setting-row`. A regression in the iteration / filtering would silently
    // empty this page out, so this asserts both shape (rows render) and content (a known
    // entry surfaces with its label).
    const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('Advanced'))
    expect(clicked).toBe(true)

    await settings.waitForSelector('[data-section-id="advanced"] .advanced-setting-row', 3000)
    const probe = await settings.evaluate<{ rowCount: number; names: string[] }>(
      `(function() {
        var rows = Array.from(document.querySelectorAll('[data-section-id="advanced"] .advanced-setting-row'));
        var names = rows.map(function(r) {
          var name = r.querySelector('.setting-name');
          return name ? (name.textContent || '').replace(/●/g, '').trim() : '';
        });
        return { rowCount: rows.length, names: names };
      })()`,
    )
    // 16 entries today; assert >= 10 so adding/removing a handful doesn't churn the test.
    expect(probe.rowCount).toBeGreaterThanOrEqual(10)
    // Sample entries from both the genuinely-Advanced bucket and the ones that surface
    // there via `showInAdvanced: true` despite living under another section path.
    expect(probe.names).toContain('Prefetch buffer size')
    expect(probe.names).toContain('Maximum conflicts to show')
    expect(probe.names).toContain('Progress update interval')
  })

  test('search narrows the visible sidebar sections and clearing restores them', async () => {
    // Capture the baseline section count with no filter applied.
    await settings.waitForSelector('.section-item', 5000)
    const baselineCount = await settings.count('.section-item')
    expect(baselineCount).toBeGreaterThan(2)

    // Filter to a narrow query (only Appearance-tied options should match).
    await settings.fill('.search-input', 'accent')

    // Sidebar updates on a 200 ms debounce: wait for the count to drop.
    await expect
      .poll(
        async () => {
          const count = await settings.count('.section-item')
          return count > 0 && count < baselineCount
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

    // Clear button must reset both the input and the visible section list.
    await settings.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)

    await expect
      .poll(
        async () => {
          const count = await settings.count('.section-item')
          const value = await settings.inputValue('.search-input')
          return count === baselineCount && value === ''
        },
        { timeout: 3000 },
      )
      .toBeTruthy()
  })

  test('search shows an empty sidebar for queries with no matches', async () => {
    await settings.waitForSelector('.section-item', 5000)
    await settings.fill('.search-input', 'zzzyyyxxxnomatch')

    // Sidebar updates on a 200 ms debounce: wait for all sections to vanish.
    await expect.poll(async () => (await settings.count('.section-item')) === 0, { timeout: 3000 }).toBeTruthy()

    // The clear button still shows up so the user can recover from a dead-end query.
    expect(await settings.isVisible('.search-clear')).toBe(true)

    // Reset state for the next test in the file.
    await settings.evaluate(`(function() {
            var btn = document.querySelector('.search-clear');
            if (btn) btn.click();
        })()`)
    await expect.poll(async () => (await settings.count('.section-item')) > 0, { timeout: 3000 }).toBeTruthy()
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
    await expect.poll(async () => (await settings.count('.section-item')) > 2, { timeout: 3000 }).toBeTruthy()

    // Reset the selected section to the first sidebar entry. Prior tests may
    // have landed on the last entry (post-reorg, that's `Advanced`), where
    // ArrowDown is a no-op by design (`navigateSections` clamps at the end of
    // `allSections`). Without this reset the test reads "no change" and fails
    // even though the keyboard handler is wired correctly.
    await settings.evaluate(`(function() {
            var items = document.querySelectorAll('.section-item');
            if (items[0]) items[0].click();
        })()`)
    await expect
      .poll(
        async () => {
          const cls = await settings.evaluate<string>(
            `document.querySelectorAll('.section-item')[0]?.getAttribute('class') || ''`,
          )
          return cls.includes('selected')
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

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

    await expect
      .poll(
        async () => {
          const now = await settings.evaluate<string>(
            `document.querySelector('.section-item.selected')?.textContent?.trim() || ''`,
          )
          return now !== startSelected && now !== ''
        },
        { timeout: 3000 },
      )
      .toBeTruthy()
  })
})

test.describe('Settings keyboard-shortcut deep link', () => {
  // A clickable `ShortcutChip` opens Settings > Keyboard shortcuts scrolled to a
  // specific command's row, with a brief flash. `openShortcutCustomization` builds
  // the URL `/settings?section=["Keyboard shortcuts"]&anchor=shortcut-<commandId>`,
  // exactly what we create here directly (the same `plugin:webview` IPC the prod
  // `new WebviewWindow` call uses), so this exercises the settings-side arrival
  // logic: URL-anchor parse → filter clear → mount the row → scroll the nested
  // `.commands-list` to it.
  //
  // Opened with `focus: false` like prod does in E2E, so this also guards the
  // unfocused-window rAF-throttle rule (the scroll/flash uses `setTimeout(0)`, not
  // `requestAnimationFrame`). See `docs/testing.md` § "rAF in unfocused windows".

  let settings: TauriPage

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, settings, 'settings')
  })

  test('lands on Keyboard shortcuts with the target row scrolled into view', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    // `file.quickLook` is the Quick Look toast's deep-link target. Its scope is the
    // compound `Main window/File list`, so this also guards that compound-scope rows
    // render and are reachable (the section groups every command by scope; see
    // `sections/CLAUDE.md` § "Every command groups by scope"). The row lives far
    // enough down the list to need a scroll, exercising the nested `.commands-list`
    // scroll path.
    const targetAnchor = 'shortcut-file.quickLook'
    const section = JSON.stringify(['Keyboard shortcuts'])
    const url = `/settings?section=${encodeURIComponent(section)}&anchor=${encodeURIComponent(targetAnchor)}`
    const urlJson = JSON.stringify(url)

    await main.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        return invoke('plugin:webview|create_webview_window', {
            options: {
                label: 'settings',
                url: ${urlJson},
                title: 'Settings',
                width: 800, height: 600, minWidth: 400, minHeight: 300,
                resizable: true, focus: false,
            },
        });
    })()`)

    settings = await main.waitForWindow((w) => w.label === 'settings', { timeout: 10000 })
    await settings.waitForSelector('.settings-window', 3000)
    // The Keyboard-shortcuts section renders the shortcut table; wait for the
    // target row to exist (the deep link clears any filter that might hide it).
    // Attribute selector, not `#id`: the anchor contains a `.` (`downloads.goToLatest`)
    // which a CSS `#` selector would parse as a class.
    await settings.waitForSelector(`[id="${targetAnchor}"]`, 3000)

    // The row must be scrolled into the nested `.commands-list` viewport. Poll the
    // rect comparison: the deep-link scroll defers via `setTimeout(0)`, so the row
    // may start out of view and settle into it. `expect.poll` makes the wait the
    // assertion (no bare `pollUntil` that could pass silently on timeout).
    await expect
      .poll(
        () =>
          settings.evaluate<boolean>(
            `(function() {
              var row = document.getElementById(${JSON.stringify(targetAnchor)});
              var list = row && row.closest('.commands-list');
              if (!row || !list) return false;
              var r = row.getBoundingClientRect();
              var l = list.getBoundingClientRect();
              // Fully within the inner scroller's viewport (small slack for borders).
              return r.top >= l.top - 1 && r.bottom <= l.bottom + 1;
            })()`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()
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

    // Dispatch a synthetic Escape keydown into the settings webview rather
    // than going through Playwright's OS-level `keyboard.press`. The handler
    // we want to exercise is the JS `<svelte:window on:keydown>` in
    // `routes/settings/+page.svelte` — bubbling from `document` fires that
    // listener regardless of OS focus. Going through the OS path adds a
    // focus dependency that flakes under Xvfb on Linux CI (the keystroke
    // lands on the main window and the settings webview never sees it).
    // The Tauri/webkit2gtk OS → webview event pipeline isn't cmdr's
    // responsibility to test; this binding is.
    //
    // Fire-and-forget: the dispatched Escape triggers
    // `getCurrentWindow().close()` (after a 2-rAF defer) inside the handler,
    // so the window may die before the evaluate promise's `pw_result` fires
    // back. The poll on `listWindows()` below is the assertion that matters.
    settings
      .evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`)
      .catch(() => {
        /* window died mid-script before pw_result; expected */
      })

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
