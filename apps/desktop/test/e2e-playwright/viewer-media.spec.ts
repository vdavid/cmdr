/**
 * E2E tests for the file viewer's media (image / PDF) rendering.
 *
 * Opens a tiny fixture PNG and a 1-page fixture PDF through the production
 * multi-window flow (same as `viewer.spec.ts`) and asserts:
 *   - the image `<img>` actually decoded under the CSP (`naturalWidth > 0`),
 *   - the binary-warning banner is ABSENT (media renders inline now),
 *   - no CSP-violation fired (the most likely failure if the `cmdr-media:` CSP
 *     token / scheme origin is wrong).
 *
 * The PDF `<embed>` can't expose `naturalWidth`, so its load is asserted via
 * the embed's presence paired with the banner-absent + no-CSP-error checks.
 *
 * Fixtures: `left/sample.png` (2x2 RGBA) and `left/sample.pdf` (1 page), shipped
 * by `e2e-shared/fixtures.ts`.
 */

import path from 'path'
import { test, expect } from './fixtures.js'
import {
  closeScopedWindow,
  openViewerWindow,
  pointerClick,
  runAnimationFramesOnTimers,
  selectContentExpr,
  selectRowHits,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const fixtureRoot = (() => {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root)
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  return root
})()
const pngPath = path.join(fixtureRoot, 'left', 'sample.png')
const pdfPath = path.join(fixtureRoot, 'left', 'sample.pdf')

/**
 * Opens a viewer for `filePath`, installs a CSP-violation recorder in the new
 * window, and waits for the viewer to finish loading. Returns the scoped page.
 *
 * The recorder is a `securitypolicyviolation` listener writing to a window
 * global; read it back later via `mediaCspViolations`. Installed as soon as the
 * window is scoped (the media request is in flight around now), so a wrong CSP
 * token surfaces as a recorded violation.
 */
async function openMediaViewer(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.evaluate(`(function () {
    window.__cmdrCspViolations = window.__cmdrCspViolations || []
    document.addEventListener('securitypolicyviolation', function (e) {
      window.__cmdrCspViolations.push(String(e.blockedURI) + ' / ' + String(e.violatedDirective))
    })
  })()`)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 8000)
  return viewer
}

/**
 * CSP violations that mention our media scheme or the directives that govern it.
 *
 * The webview fires unrelated CSP violations from the test harness itself
 * (`plugin:playwright|pw_result` and `plugin:notification` on `connect-src`, an
 * `inline / style-src-attr`); those are pre-existing and not this feature's
 * concern. We only care that the `cmdr-media://` request was NOT blocked, which
 * would show up as a violation whose blocked URI is `cmdr-media:` or whose
 * directive is `img-src` / `object-src`.
 */
async function mediaCspViolations(viewer: TauriPage): Promise<string[]> {
  const raw = await viewer.evaluate<string>(`JSON.stringify(window.__cmdrCspViolations || [])`)
  const all = JSON.parse(raw) as string[]
  return all.filter((v) => v.includes('cmdr-media') || v.includes('img-src') || v.includes('object-src'))
}

test.describe('File viewer media rendering', () => {
  test('renders a PNG inline with no banner and no CSP violation', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const viewer = await openMediaViewer(main, pngPath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      // The image decoded: naturalWidth is non-zero only after a successful load,
      // which only happens if the `cmdr-media://` request passed the CSP.
      await expect
        .poll(
          async () => {
            const w = await viewer.evaluate<number>(
              `(function () { var i = document.querySelector('.media-image'); return i ? i.naturalWidth : 0 })()`,
            )
            return w
          },
          { timeout: 5000 },
        )
        .toBeGreaterThan(0)

      // The raw-bytes banner must be absent: an image renders inline now.
      expect(await viewer.isVisible('.binary-warning')).toBe(false)
      // No virtual-scroll text content for an image.
      expect(await viewer.isVisible('.file-content')).toBe(false)

      // No CSP violation fired (the wrong-token failure mode).
      expect(await mediaCspViolations(viewer)).toEqual([])
    } finally {
      await closeScopedWindow(main, viewer, label)
    }
  })

  // The view-mode menu opens OVER its trigger and hangs down across the image. A menu
  // that lost its `--z-dropdown` rung sits at `z-index: auto`, under anything on a real
  // rung: the menu looks open, but its rows are hidden and a click lands elsewhere.
  //
  // Two things keep this test honest. Frames run on timers, because zag's placement
  // (which writes the positioner's inline stacking styles) happens in a rAF that an
  // unfocused E2E window never fires. And a full-window sentinel sits at `--z-sticky`,
  // standing in for chrome on a lower rung (a window's drag strip): the menu portals to
  // the end of `<body>`, so DOM order alone would lift even an unstacked menu over the
  // unstacked image stage.
  test('the view-mode menu opens above the image, and every row takes a click', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const viewer = await openMediaViewer(main, pngPath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      await viewer.waitForSelector('.media-image-stage', 5000)
      await runAnimationFramesOnTimers(viewer)
      await viewer.evaluate(`(function () {
        var sentinel = document.createElement('div');
        sentinel.id = 'lower-rung-sentinel';
        sentinel.style.setProperty('position', 'fixed');
        sentinel.style.setProperty('inset', '0');
        sentinel.style.setProperty('z-index', 'var(--z-sticky)');
        document.body.appendChild(sentinel);
      })()`)
      // The sentinel really answers clicks over the image, or an empty `covered` below proves nothing.
      expect(
        await viewer.evaluate<string | null>(`(function () {
          var r = document.querySelector('.media-image-stage').getBoundingClientRect();
          var hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          return hit ? hit.id : null;
        })()`),
      ).toBe('lower-rung-sentinel')

      const trigger = `document.querySelector('.viewer-toolbar-pickers .select-trigger')`
      expect(await pointerClick(viewer, trigger)).toBe('clicked')
      // Wait for zag's placement to land (it writes the positioner's inline `--x`), since that
      // same write is what sets the positioner's rung. Probing before it tests a menu no user sees.
      await expect
        .poll(
          async () =>
            viewer.evaluate<string>(`${selectContentExpr(trigger)}.parentElement.style.getPropertyValue('--x')`),
          {
            timeout: 5000,
          },
        )
        .not.toBe('')

      // Poll the hit test itself: the menu settles its overlap shift a few frames after
      // opening, and the user only ever clicks the settled menu.
      await expect
        .poll(async () => selectRowHits(viewer, trigger, '.media-image-stage'), { timeout: 5000 })
        .toEqual({ covered: [], over: ['viewAsText'] })
    } finally {
      await closeScopedWindow(main, viewer, label)
    }
  })

  test('renders a PDF inline with no banner and no CSP violation', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const viewer = await openMediaViewer(main, pdfPath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      // The PDF embed is present (its load can't expose naturalWidth, so pair it
      // with banner-absent + no-CSP-error below).
      await viewer.waitForSelector('.media-pdf', 5000)
      expect(await viewer.isVisible('.media-pdf')).toBe(true)

      expect(await viewer.isVisible('.binary-warning')).toBe(false)
      expect(await viewer.isVisible('.file-content')).toBe(false)

      expect(await mediaCspViolations(viewer)).toEqual([])
    } finally {
      await closeScopedWindow(main, viewer, label)
    }
  })
})
