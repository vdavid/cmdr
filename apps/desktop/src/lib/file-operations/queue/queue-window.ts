/**
 * Operation-queue window management.
 *
 * A real macOS vibrancy window (a sibling of Settings / Keyboard shortcuts, NOT
 * a modal): it lists every running and queued copy, move, delete, and trash
 * operation with per-row pause/resume/cancel, multi-select + "Cancel selected",
 * and global pause/resume. It's a hard window so the user can keep working in
 * the main window while operations run in the background and still manage them.
 *
 * Cloned from `lib/settings/settings-window.ts`: open-or-focus a singleton
 * `queue` window, position via the shared `#lib/window-positioning` helpers,
 * apply `NSVisualEffectView` vibrancy (with the reduce-transparency opaque
 * fallback), and use an overlay title bar whose line our own title shares with
 * the traffic lights. The opener runs on the MAIN window, which already holds the
 * window-creation / monitor perms (`adding-a-window.md`); the queue window's own
 * capability file (`capabilities/queue.json`) grants only what its page calls.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import { Effect, EffectState } from '@tauri-apps/api/window'
import { getShouldReduceTransparency, raiseChildWindow } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getEffectiveScale } from '#lib/text-size.svelte.js'
import { decorateChildWindowTitle, isE2eRun, orderChildWindowToBackInE2e } from '#lib/app-mode.js'
import { readMainRect, readMonitors, readSavedRect, resolveChildPosition } from '#lib/window-positioning.js'
import { scheduleShowFallback } from '#lib/child-window-show.js'

const log = getAppLogger('queue')

/** Base (scale = 1) window size. Roomy enough for several operation rows with
 *  inline progress bars; resizable, with the list scrolling inside.
 *
 *  The minimum is set by one row's contents, not by taste: a row's readout
 *  columns (bar label, amount, percent, rate, time left) are fixed-width, and
 *  below this the progress bars they sit beside squeeze down to nothing. Shrink
 *  the minimum only alongside `TransferProgressReadout`'s compact columns.
 *
 *  Both numbers carry 80 px for the "Bytes" / "Files" label column in front of
 *  each bar. That column is `auto`, so it costs whatever the locale's longer
 *  word measures plus one 4 px gap: 36 px in English, 70 px in Dutch
 *  ("Onderdelen", the widest of the ten locales), 71 px in the `en-XA`
 *  pseudo-locale (measured in the queue webview at `--font-size-sm`; macOS
 *  15.5, 2026-08-09). 80 px covers the worst of those with room to spare.
 *
 *  On top of the readout's floor, the minimum leaves the summary line room to
 *  say what a row acts on and where ("Copying 2 folders and 1 file Dropbox →
 *  cmdr-s3-test/…/folder"), and the initial width shows a typical destination
 *  whole.
 *
 *  The height fits one running transfer with its six-row live tree (about 245 px
 *  with its readout) under the header (the title-bar line plus the toolbar), and
 *  still shows five or so waiting rows below it, so the line behind the work
 *  stays in view (measured in a dev build at the default text size, 2026-10-12). */
const BASE_WIDTH = 960
const BASE_HEIGHT = 560
const MIN_WIDTH = 744
const MIN_HEIGHT = 280

/** The backend's (NSWorkspace) answer, opaque when it can't be read. */
async function readReduceTransparency(): Promise<boolean> {
  try {
    return await getShouldReduceTransparency()
  } catch (error) {
    log.warn('Failed to read reduce-transparency; opening opaque: {error}', { error })
    return true
  }
}

/** Clear behind the vibrancy material, or an opaque fill in the system appearance. */
function windowBackground(reduceTransparency: boolean): [number, number, number, number] {
  if (!reduceTransparency) return [0, 0, 0, 0]
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? [30, 30, 30, 255] : [255, 255, 255, 255]
}

/**
 * Opens the operation-queue window, or raises it if already open (singleton,
 * like Settings). The backend's `raiseChildWindow` restores a minimized or
 * hidden window too, which `setFocus()` silently refuses to touch.
 *
 * Both paths log at info: `queue` isn't a debug category, and a report saying
 * "the queue won't open" needs the bundle to say which path ran.
 *
 * Every Tauri call is awaited in try/catch with a `log.warn`: window perms fail
 * SILENTLY, so a missing grant must surface as a log line, not a dead window.
 */
export async function openQueueWindow(): Promise<void> {
  // E2E suites re-open windows many times; stealing OS focus each time makes the
  // host machine unusable while tests run. The plugin drives the webview over a
  // socket, so it doesn't need OS focus. Mirrors Settings / Shortcuts.
  const takesFocus = !isE2eRun()

  try {
    const existing = await WebviewWindow.getByLabel('queue')
    if (existing) {
      log.info('Raising the existing operation-queue window')
      if (takesFocus) {
        await raiseChildWindow('queue')
      }
      return
    }
  } catch (error) {
    log.warn('Failed to raise the existing queue window: {error}', { error })
    return
  }

  log.info('Creating new operation-queue window')

  const scale = getEffectiveScale()
  const width = BASE_WIDTH * scale
  const height = BASE_HEIGHT * scale

  // Pick a position: saved-and-on-screen, else clamped, else centered on main.
  const [main, monitors, saved] = await Promise.all([readMainRect(), readMonitors(), readSavedRect('queue')])
  const rect = main ? resolveChildPosition({ size: { width, height }, main, monitors, saved }) : null

  // Honor macOS "Reduce transparency": open an opaque window and skip the
  // vibrancy material (the page tokens flip to opaque under the
  // `reduce-transparency` class). Read the value from the backend (NSWorkspace),
  // NOT a media query: WKWebView doesn't reflect `prefers-reduced-transparency`.
  // `prefers-color-scheme` IS reflected, so dark detection stays a media query.
  const reduceTransparency = await readReduceTransparency()
  const backgroundColor = windowBackground(reduceTransparency)

  const win = new WebviewWindow('queue', {
    url: '/queue',
    title: decorateChildWindowTitle(tString('queue.windowTitle')),
    width: rect?.width ?? width,
    height: rect?.height ?? height,
    minWidth: MIN_WIDTH * scale,
    minHeight: MIN_HEIGHT * scale,
    ...(rect ? { x: rect.x, y: rect.y } : { center: true }),
    resizable: true,
    minimizable: true,
    closable: true,
    decorations: true,
    focus: takesFocus,
    // Created hidden; the queue page shows it, sliding in, once it's ready
    // (`#lib/child-window-show`). E2E keeps the immediate show.
    visible: !takesFocus,
    // Translucent glass backdrop via the macOS `NSVisualEffectView` material,
    // UNLESS the user reduces transparency (then open opaque). Requires
    // `tauri/macos-private-api` (enabled in `Cargo.toml`).
    transparent: !reduceTransparency,
    backgroundColor,
    // Overlay title bar with the traffic lights where the main window has them,
    // so the page paints its title on their line like a native window title.
    // `hiddenTitle` keeps the OS from painting the window title text.
    titleBarStyle: 'overlay',
    hiddenTitle: true,
    trafficLightPosition: new LogicalPosition(9, 17),
  })

  // A refused creation doesn't throw here: it arrives afterwards as `tauri://error`,
  // so without this a missing grant leaves no trace at all.
  void win.once('tauri://error', (event) => {
    log.warn("Couldn't create the queue window: {error}", { error: event.payload })
  })
  if (takesFocus) scheduleShowFallback('queue')

  // Apply the `NSVisualEffectView` material AFTER creation (the `windowEffects`
  // creation option drops silently in this Tauri version; `setEffects` is the
  // reliable IPC path, gated by `core:window:allow-set-effects` in
  // `capabilities/queue.json`). UnderWindowBackground reads as a clean utility
  // "HUD-ish" panel — the macOS convention for a transfer/activity manager —
  // and follows the window's active state. Radius matches `--radius-window`
  // (the file viewer's stock corner), like every other child window.
  if (!reduceTransparency) {
    void win.once('tauri://created', () => {
      void win
        .setEffects({
          effects: [Effect.UnderWindowBackground],
          state: EffectState.FollowsWindowActiveState,
          radius: 16,
        })
        .catch((error: unknown) => {
          log.warn('Failed to apply queue window effects: {error}', { error })
        })
    })
  }

  // E2E: push the window behind everything so a run's windows don't pop in front
  // of the developer's work. No-op outside E2E.
  void orderChildWindowToBackInE2e(win)
}
