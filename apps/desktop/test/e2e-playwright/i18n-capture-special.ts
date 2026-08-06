/**
 * Special-surface capture functions for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`): the report/feedback/license dialogs, the file-viewer
 * subsurfaces, and the license-state surfaces that need a separate
 * `CMDR_MOCK_LICENSE` launch.
 *
 * Kept separate from `i18n-capture-surfaces.ts` (the original surface groups)
 * both for the file-length budget and because these share a theme: surfaces that
 * need a command trigger, a different window/file type, or a launch-time env the
 * frontend can't toggle. All use the shared engines in `i18n-capture-helpers.ts`.
 */

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from './fixtures.js'
import {
  ensureAppReady,
  dismissOverlay,
  openViewerWindow,
  closeScopedWindow,
  dispatchMenuCommand,
  getFixtureRoot,
  CTRL_OR_META,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface, focusWindow } from './i18n-capture-helpers.js'

/**
 * Captures the main-window report/feedback/license dialogs reachable on the
 * DEFAULT launch (no license mock): the license-key ENTRY dialog (Personal
 * state, no key on file), the error-report dialog, the feedback dialog, and the
 * acknowledgements dialog.
 *
 * All are `ModalDialog`s rendered into the MAIN window's sink and opened by a
 * registry command, so each follows the About rhythm: enable + setSurface the
 * sink BEFORE dispatching the command (to record mount-time `t()` calls), wait on
 * the dialog's `data-dialog-id`, capture, then dismiss + disable.
 *
 * The COMMERCIAL / perpetual / expired / reminder license surfaces live in their
 * own `CMDR_MOCK_LICENSE` launches (`captureLicensePass` in
 * `i18n-capture-staged.ts`): `app_status.rs` reads that env only under
 * `#[cfg(debug_assertions)]`, which the capture build turns on for the release
 * profile. The license DETAILS view stays document-skipped in the spec: it needs
 * a real committed key, which the `AppStatus` mock doesn't populate.
 */
export async function captureMainDialogs(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)

  // Opens one main-window ModalDialog by command, captures it, dismisses it.
  // `waitSelector` should name something the dialog only renders once it has the
  // content its copy describes, not just its shell. It's handed back as the
  // `readySelector` too, so the same condition is re-proven in the frame that
  // actually gets photographed rather than only at open time.
  const dialogSurface = async (label: string, commandId: string, waitSelector: string): Promise<void> => {
    await captureSurface(label, report, failed, async () => {
      await captureCall(main, 'reset')
      await captureCall(main, 'setSurface', label)
      await captureCall<boolean>(main, 'enable')
      await dispatchMenuCommand(main, commandId)
      await main.waitForSelector(waitSelector, 5000)
      return { page: main, readySelector: waitSelector }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
  }

  // License-key ENTRY dialog (`app.licenseKey`). On the default (Personal) launch
  // no key is on file, so the dialog opens in entry mode (input + activate),
  // rendering the `licensing.dialog.enter*` / `inputPlaceholder` / `activate` keys
  // and the error-code copy. The DETAILS view (a committed license) needs the
  // commercial mock, captured by the license pass.
  await dialogSurface('license-key-dialog', 'app.licenseKey', '[data-dialog-id="license"]')

  // Error-report dialog (`help.sendErrorReport`). Opens directly from the command
  // with no real error needed; renders the `errorReporter.dialog.*` copy.
  await dialogSurface('error-report', 'help.sendErrorReport', '[data-dialog-id="error-report"]')

  // Feedback dialog (`feedback.send`). Opens directly; renders `feedback.dialog.*`.
  await dialogSurface('feedback', 'feedback.send', '[data-dialog-id="feedback"]')

  // Acknowledgements dialog (`app.acknowledgements`). It loads the generated
  // third-party notices from an `import()` that settles over an unknown number of
  // macrotasks, showing a spinner until they land, so we gate on a real PACKAGE
  // ROW: the loaded branch is what renders the jump links, section headings, and
  // the full-texts line (`licensing.acknowledgements.*`).
  //
  // ❗ `.packages-scroll` is NOT enough, even though it lives in the loaded branch:
  // it once passed while the shot still caught "Loading the list…". `.package-list
  // li` is the signal the component's own tests poll for (`licensing/DETAILS.md` §
  // Testing gotcha), and `captureSurface` re-checks it at shot time.
  await dialogSurface(
    'acknowledgements',
    'app.acknowledgements',
    '[data-dialog-id="acknowledgements"] .package-list li',
  )
}

/** Files per staged source dir: enough that a copy stays Running through the shot. */
const QUEUE_FILES_PER_SOURCE = 24
/** Two distinct sources so the copies don't collide on the destination; both sit on
 *  volume `root`, so they share the local lane and serialize (one Running, one Queued). */
const QUEUE_SOURCES = ['queue-shot-a', 'queue-shot-b']
/** Per-file delay (ms) the backend honors while capturing, so the transfers outlive the shot. */
const QUEUE_THROTTLE_MS = 250

/** Creates `left/<name>/` with `QUEUE_FILES_PER_SOURCE` tiny files, Node-side on real disk. */
function makeQueueSource(fixtureRoot: string, name: string): void {
  const dir = join(fixtureRoot, 'left', name)
  mkdirSync(dir, { recursive: true })
  for (let i = 0; i < QUEUE_FILES_PER_SOURCE; i++) {
    writeFileSync(join(dir, `file-${String(i).padStart(2, '0')}.txt`), 'x'.repeat(1024))
  }
}

/** Starts a local→local copy of `left/<sourceName>/` into `right/` through the production IPC. */
async function startQueueCopy(main: TauriPage, fixtureRoot: string, sourceName: string): Promise<void> {
  const src = JSON.stringify(join(fixtureRoot, 'left', sourceName))
  const destDir = JSON.stringify(join(fixtureRoot, 'right'))
  await main.evaluate(`window.__TAURI_INTERNALS__.invoke('copy_between_volumes', {
    sourceVolumeId: 'root',
    sourcePaths: [${src}],
    destVolumeId: 'root',
    destPath: ${destDir},
    config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] }
  })`)
}

/**
 * Captures the TRANSFER QUEUE window in both of its states: empty, then with one
 * Running and one Queued row.
 *
 * `/queue` is its own `WebviewWindow` (own webview context, own capture sink),
 * opened by the `queue.show` registry command, and every `queue.*` key is
 * exclusive to it: nothing else in the app can stand in for this surface.
 *
 * The rows need real work in flight, so this stages the same two same-lane copies
 * `transfer-queue.spec.ts` uses and slows the backend with `set_test_throttle` so
 * they outlive the screenshot. Empty first, so `queue.empty.*` couples to the
 * image that actually shows the empty state rather than to a populated list.
 *
 * The throttle is cleared and every staged operation cancelled in `finally`, so a
 * later surface never captures a queue still grinding through leftovers.
 */
export async function captureQueueWindow(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)
  const fixtureRoot = getFixtureRoot()
  for (const name of QUEUE_SOURCES) makeQueueSource(fixtureRoot, name)

  let queue: TauriPage | undefined
  try {
    await dispatchMenuCommand(main, 'queue.show')
    queue = await main.waitForWindow((w) => w.label === 'queue', { timeout: 10000 })
    const q = queue

    await captureSurface('queue-empty', report, failed, async () => {
      // `waitForWindow` returns the moment the label exists, which is BEFORE the
      // document loads; an eval landing in the outgoing document is torn down
      // before it can post its result, so one retry rides that out.
      await q.waitForSelector('.empty-state', 15000).catch(async () => {
        await q.waitForSelector('.empty-state', 15000)
      })
      await focusWindow(q, 'queue')
      await captureCall(q, 'reset')
      await captureCall<boolean>(q, 'enable')
      return { page: q, focusLabel: 'queue' }
    })

    await captureSurface('queue', report, failed, async () => {
      await main.evaluate(
        `window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: ${String(QUEUE_THROTTLE_MS)} })`,
      )
      for (const name of QUEUE_SOURCES) await startQueueCopy(main, fixtureRoot, name)
      // Both rows present AND settled into their two different statuses: the
      // Queued row's copy (`queue.row.queued`) is half of what this surface adds.
      await expect
        .poll(
          async () =>
            q.evaluate<string>(`(function(){
              var rows = Array.from(document.querySelectorAll('.queue-row'));
              return rows.map(function(r){ return r.getAttribute('data-status'); }).sort().join(',');
            })()`),
          { timeout: 20000 },
        )
        .toBe('queued,running')
      return { page: q, focusLabel: 'queue' }
    })
  } catch (err) {
    for (const label of ['queue-empty', 'queue']) {
      if (!(label in report) && !failed.includes(label)) failed.push(label)
    }
    console.warn(`[i18n-capture] queue window setup FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    await main
      .evaluate(`(async function(){
        try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
        try {
          var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
          var ids = ops.map(function(o){ return o.operationId; });
          if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
        } catch (e) {}
      })()`)
      .catch(() => {})
    if (queue) await closeScopedWindow(main, queue, 'queue').catch(() => {})
  }
}

/**
 * Captures the FILE VIEWER subsurfaces, each in its own viewer window.
 *
 * The base `viewer` surface (default text chrome) is captured by the spec. This
 * adds the states that need a trigger or a different file type:
 *  - `viewer-search`: the find bar (⌘F / Ctrl+F inside the viewer).
 *  - `viewer-context-menu`: the right-click menu on `.file-content`.
 *  - `viewer-view-mode` / `viewer-encoding`: the toolbar Select dropdowns
 *    (their group labels + items only mount while open).
 *  - `viewer-image` / `viewer-pdf`: media rendering, opened on the committed
 *    `sample.png` / `sample.pdf` fixtures (`createFixtures` copies them to
 *    `left/`).
 *
 * Each viewer state opens a fresh viewer window (own webview context + sink),
 * focuses it (occluded child windows throttle paint), captures, and closes it.
 * Per-surface isolation via `captureSurface` means one viewer state failing
 * doesn't stop the rest. The text-file states reuse the base text fixture; the
 * media states open their own typed fixture.
 */
export async function captureViewerSubsurfaces(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  skipped: string[],
): Promise<void> {
  const startRoot = process.env.CMDR_E2E_START_PATH
  if (!startRoot) {
    for (const label of ['viewer-search', 'viewer-context-menu', 'viewer-view-mode', 'viewer-encoding']) {
      if (!failed.includes(label)) failed.push(label)
    }
    console.warn('[i18n-capture] viewer subsurfaces: CMDR_E2E_START_PATH unset; cannot resolve fixtures')
    return
  }
  const textFixture = join(startRoot, 'left', 'file-a.txt')
  const imageFixture = join(startRoot, 'left', 'sample.png')
  const pdfFixture = join(startRoot, 'left', 'sample.pdf')

  // Opens a viewer window on `filePath`, runs `prep` (the surface-specific
  // trigger), captures under `label`, and closes the window. Each window has its
  // own capture sink, reset+enabled here.
  const viewerSurface = async (
    label: string,
    filePath: string,
    readySelector: string,
    prep: (viewer: TauriPage) => Promise<void>,
  ): Promise<void> => {
    let viewer: TauriPage | undefined
    let viewerLabel: string | undefined
    await captureSurface(label, report, failed, async () => {
      viewer = await openViewerWindow(main, filePath)
      viewerLabel = viewer.targetWindow
      if (!viewerLabel) throw new Error('viewer page has no targetWindow label')
      const v = viewer
      await v.waitForSelector('.viewer-container[data-window-ready="loaded"]', 15000)
      await focusWindow(v, viewerLabel)
      await captureCall(v, 'reset')
      await captureCall<boolean>(v, 'enable')
      await prep(v)
      await v.waitForSelector(readySelector, 5000)
      return { page: v, focusLabel: viewerLabel }
    })
    if (viewer && viewerLabel) await closeScopedWindow(main, viewer, viewerLabel).catch(() => {})
  }

  // Find bar: ⌘F / Ctrl+F opens the in-file search. Renders `viewer.search.*`.
  await viewerSurface('viewer-search', textFixture, '.search-bar input.text-field-control', async (v) => {
    await v.evaluate(`(function(){
      var el = document.querySelector('.file-content') || document.body;
      el.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'f', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, bubbles: true
      }));
    })()`)
  })

  // Context menu: right-click `.file-content`. Renders `viewer.contextMenu.*`.
  await viewerSurface('viewer-context-menu', textFixture, '.viewer-context-menu', async (v) => {
    await v.evaluate(`(function(){
      var el = document.querySelector('.file-content');
      if (el) el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, button: 2, clientX: 80, clientY: 80 }));
    })()`)
  })

  // View-mode + encoding picker dropdowns (the two toolbar `Select`s). Their group
  // labels + items (`viewer.toolbar.viewMode.*` / `viewer.kind.*` /
  // `viewer.toolbar.encoding.*`) mount in `.select-content` only while OPEN. Ark UI
  // opens on `pointerdown`, not a synthetic `click`, so the trigger gets a full
  // pointer sequence. BEST-EFFORT: if the dropdown still doesn't open under the
  // occluded-child-window native eval, the surface is moved from `failed` to
  // `skipped` (the trigger copy is already on the base `viewer` surface). The
  // `nth` picks the view-mode trigger (first) vs the encoding trigger (last).
  const openPicker = (nth: 'first' | 'last') => `(function(){
    var triggers = document.querySelectorAll('.viewer-toolbar-pickers .select-trigger');
    var t = ${nth === 'first' ? 'triggers[0]' : 'triggers[triggers.length - 1]'};
    if (!t) return;
    var opts = { bubbles: true, cancelable: true, button: 0, pointerId: 1, pointerType: 'mouse', isPrimary: true };
    t.dispatchEvent(new PointerEvent('pointerdown', opts));
    t.dispatchEvent(new PointerEvent('pointerup', opts));
    t.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  })()`
  await viewerSurface('viewer-view-mode', textFixture, '.select-content', async (v) => {
    await v.evaluate(openPicker('first'))
  })
  await viewerSurface('viewer-encoding', textFixture, '.select-content', async (v) => {
    await v.evaluate(openPicker('last'))
  })
  // Demote any picker that couldn't open from a hard failure to a documented skip,
  // so a flaky Ark dropdown can't fail the whole refresh.
  for (const label of ['viewer-view-mode', 'viewer-encoding']) {
    const idx = failed.indexOf(label)
    if (idx >= 0) {
      failed.splice(idx, 1)
      if (!skipped.includes(label)) skipped.push(label)
      console.warn(
        `[i18n-capture] ${label} SKIPPED: the Ark Select dropdown didn't open in the occluded viewer ` +
          `window (its trigger copy is already on the base \`viewer\` surface).`,
      )
    }
  }

  // Image media view: open `sample.png`. Renders `viewer.image.*` /
  // `viewer.media.*` / the image status-bar hint. The image element mounts inside
  // `.media-image`.
  await viewerSurface('viewer-image', imageFixture, '.media-image', async () => {
    // No trigger: opening the image fixture renders media mode directly.
  })

  // PDF media view: open `sample.pdf`. Renders `viewer.pdf.*` / `viewer.media.*`.
  await viewerSurface('viewer-pdf', pdfFixture, '.media-pdf', async () => {
    // No trigger: opening the PDF fixture renders media mode directly.
  })
}
