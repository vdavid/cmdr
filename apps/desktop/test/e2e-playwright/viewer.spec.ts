/**
 * E2E tests for the file viewer.
 *
 * The viewer runs in its own Tauri window in production (label `viewer-<ts>`).
 * Each test opens a viewer via the production trigger (`open-file-viewer`
 * Tauri event → `openFileViewer(path)` → new `WebviewWindow`), then scopes a
 * `TauriPage` to the new window via `tauriPage.waitForWindow()`. All viewer
 * interactions go through that scoped page. The scoped page shares the plugin
 * socket with the main page, so it's cheap to create.
 *
 * Test file: a 1 KB text file from the shared E2E fixtures (`left/file-a.txt`).
 */

import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow, pollUntil } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

// Use fixture file from the shared E2E fixture tree
const fixtureRoot = process.env.CMDR_E2E_START_PATH ?? '/tmp/cmdr-e2e-fallback'
const testFilePath = path.join(fixtureRoot, 'left', 'file-a.txt')

/**
 * Opens a viewer for `filePath` and waits for the file content to render.
 * Returns the scoped TauriPage for the new viewer window.
 */
async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  // 3 s: the viewer window mounts and renders content well under 1 s on a
  // healthy machine. The previous 15 s / 10 s budgets exceeded the suite's 8 s
  // per-test ceiling and just hid failures behind the outer timeout.
  await viewer.waitForSelector('.viewer-container', 3000)
  try {
    await viewer.waitForSelector('.file-content', 3000)
  } catch {
    const hasStatusMsg = await viewer.isVisible('.status-message')
    if (hasStatusMsg) {
      const text = await viewer.textContent('.status-message')
      throw new Error(`Viewer did not load file content. Status: "${text ?? ''}"`)
    }
    throw new Error('Viewer did not load file content and no status message found')
  }
  return viewer
}

test.describe('File viewer', () => {
  let viewer: TauriPage
  let viewerLabel: string

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, testFilePath)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  test('renders the viewer container', async () => {
    expect(await viewer.isVisible('.viewer-container')).toBe(true)
  })

  test('displays file content with line elements', async () => {
    expect(await viewer.isVisible('.file-content')).toBe(true)
    const lineCount = await viewer.count('.line')
    expect(lineCount).toBeGreaterThan(0)
  })

  test('shows file name in status bar', async () => {
    const statusText = await viewer.textContent('.status-bar')
    expect(statusText).toContain('file-a.txt')
  })

  test('shows line count in status bar', async () => {
    const statusText = await viewer.textContent('.status-bar')
    // file-a.txt contains 1024 bytes of 'A' (no newlines) = 1 line
    expect(statusText).toContain('1 line')
  })

  test('shows file size in status bar', async () => {
    const statusText = await viewer.textContent('.status-bar')
    // file-a.txt is 1024 bytes = 1 KB
    expect(statusText).toContain('KB')
  })

  test('shows backend mode badge', async () => {
    expect(await viewer.isVisible('.backend-badge')).toBe(true)
    const badgeText = await viewer.textContent('.backend-badge')
    expect(badgeText).toBe('in memory')
  })
})

test.describe('File viewer search', () => {
  let viewer: TauriPage
  let viewerLabel: string

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, testFilePath)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  test('opens search bar with Ctrl+F', async () => {
    await viewer.keyboard.press('Control+f')

    await viewer.waitForSelector('.search-bar', 5000)
    expect(await viewer.isVisible('.search-bar')).toBe(true)
  })

  test('finds matches in file content', async () => {
    if (!(await viewer.isVisible('.search-bar'))) {
      await viewer.keyboard.press('Control+f')
      await viewer.waitForSelector('.search-bar', 5000)
    }
    await viewer.waitForSelector('.search-input', 5000)
    await viewer.fill('.search-input', 'AAA')

    // Wait for search results (debounced search + backend poll)
    await pollUntil(
      viewer,
      async () => {
        const visible = await viewer.isVisible('.match-count')
        if (!visible) return false
        const text = await viewer.textContent('.match-count')
        return text?.includes('of') ?? false
      },
      5000,
    )

    const matchText = await viewer.textContent('.match-count')
    expect(matchText).toContain('of')
  })

  test('closes search with Escape', async () => {
    if (!(await viewer.isVisible('.search-bar'))) {
      await viewer.keyboard.press('Control+f')
      await viewer.waitForSelector('.search-bar', 5000)
    }
    expect(await viewer.isVisible('.search-bar')).toBe(true)

    await viewer.keyboard.press('Escape')

    await pollUntil(viewer, async () => !(await viewer.isVisible('.search-bar')), 3000)
    expect(await viewer.isVisible('.search-bar')).toBe(false)
  })

  test('shows "No matches" status for a query with no hits', async () => {
    if (!(await viewer.isVisible('.search-bar'))) {
      await viewer.keyboard.press('Control+f')
      await viewer.waitForSelector('.search-bar', 5000)
    }

    await viewer.waitForSelector('.search-input', 5000)
    await viewer.fill('.search-input', 'Z'.repeat(40))

    // file-a.txt is 1024 'A' chars, so 'Z' x 40 cannot match.
    const settled = await pollUntil(
      viewer,
      async () => {
        const text = (await viewer.textContent('.match-count')) ?? ''
        return text.includes('No matches')
      },
      5000,
    )
    expect(settled).toBe(true)

    // Cleanup: clear the query.
    await viewer.fill('.search-input', '')
  })
})

test.describe('File viewer keyboard binding', () => {
  // The shared `closeScopedWindow` helper deliberately bypasses the keyboard
  // pathway (it invokes `plugin:window|close` from the main page) because the
  // scoped page's `evaluate` deadlocks when the target window dies mid-script.
  // That keeps the bulk of the suite stable but leaves the actual Escape → close
  // binding untested. This block exists to cover that binding separately.

  test('Escape closes the viewer window (production binding)', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const viewer = await openViewerForFile(main, testFilePath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    // Verify focus is inside the viewer webview before pressing Escape. If the
    // keystroke lands on the main window (focus race with the open animation,
    // or a late-mounting modal stealing focus), the viewer never receives it
    // and the test sits waiting for the window-close. Two attempts max.
    const tryEscape = async (): Promise<boolean> => {
      const focused = await pollUntil(
        viewer,
        async () =>
          viewer.evaluate<boolean>(`(function(){
            if (!document.hasFocus()) return false;
            var root = document.querySelector('.viewer-container');
            return !!(root && document.activeElement && root.contains(document.activeElement));
          })()`),
        1000,
      )
      if (!focused) {
        await viewer.evaluate(`(function(){
          var root = document.querySelector('.viewer-container');
          if (root && 'focus' in root) root.focus();
        })()`)
        return false
      }
      // Fire-and-forget: the eval that dispatches Escape may not resolve if the
      // window dies before pw_result fires back to the test runner. The
      // closeWindow() path uses two rAFs before calling .close(), so in practice
      // the eval usually resolves first — but defending against either ordering
      // keeps the test deterministic. We assert on the windowDisappeared, not
      // on the press itself.
      viewer.keyboard.press('Escape').catch(() => {
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
        return !labels.includes(label)
      },
      3000,
    )
    if (!gone) {
      throw new Error(`Escape did not close viewer window '${label}' within 3s`)
    }
  })
})

test.describe('File viewer error handling', () => {
  test('shows error for missing file path', async ({ tauriPage }) => {
    // The production `openFileViewer` helper requires a path, and the
    // `open-file-viewer` event listener routes empty paths to "open for
    // cursor" (so emitting it with `path:''` would just open file-a.txt).
    // To exercise the route guard in routes/viewer/+page.svelte we
    // directly invoke `plugin:webview|create_webview_window` with
    // `?path=` (empty) — the same IPC the prod `new WebviewWindow(...)`
    // call uses. The main window's default capability grants
    // `core:webview:allow-create-webview-window`.
    const main = tauriPage as TauriPage
    const label = `viewer-${String(Date.now())}-missing-path-test`
    const labelJson = JSON.stringify(label)
    const before = new Set((await main.listWindows()).map((w) => w.label))
    await main.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        return invoke('plugin:webview|create_webview_window', {
            options: {
                label: ${labelJson},
                url: '/viewer?path=',
                title: 'Viewer',
                width: 800, height: 600, minWidth: 400, minHeight: 300,
                resizable: true, minimizable: true, maximizable: true,
                closable: true, focus: true,
            },
        });
    })()`)
    const viewer = await main.waitForWindow((w) => w.label === label && !before.has(w.label), { timeout: 10000 })

    try {
      // 3 s budget per wait: the viewer route mounts and the missing-path branch
      // resolves well under 1 s on a healthy machine. Previous 15 s / 10 s values
      // exceeded the suite's 8 s per-test ceiling.
      await viewer.waitForSelector('.viewer-container', 3000)
      await viewer.waitForSelector('.status-message', 3000)

      // Status starts as "Loading…" before the missing-path branch resolves; poll the textContent so we read it after it
      // settles rather than the moment the element first exists.
      const settled = await pollUntil(
        viewer,
        async () => {
          const t = await viewer.textContent('.status-message')
          return t !== null && t.includes('No file path')
        },
        3000,
      )
      expect(settled).toBe(true)
    } finally {
      await closeScopedWindow(main, viewer, label)
    }
  })
})
