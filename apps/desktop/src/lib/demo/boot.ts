/**
 * The browser demo's stand-in for the Tauri backend: `hooks.client.ts` imports it first, and in
 * demo builds (`__CMDR_DEMO_BUILD__`) it installs itself before any app module runs. Every other
 * build never sees this module: `vite.config.js` aliases it to the empty `boot-absent.ts` (see
 * `DETAILS.md` § Keeping the demo out of the app); the guard covers a test that imports it directly.
 *
 * The top frame creates the page's one backend and a webview over it. A secondary window (a
 * same-origin iframe of its route, named after its window label) finds the parent's backend and
 * installs only a webview. The demo itself may be framed by another page (the landing page's
 * embed): a parent without a backend, or a cross-origin one, makes this frame the top.
 */
import { createDemoBackend, type DemoBackend } from './backend.js'
import { installMenuBar } from './menu-bar.svelte.js'
import { exposeUnanswered } from './unanswered.js'
import { installWebview } from './webview.js'

interface DemoHost {
  __cmdrDemoBackend?: DemoBackend
}

/** The parent frame's backend, when this frame is one of the demo's own secondary windows. */
function parentBackend(): DemoBackend | undefined {
  if (window.parent === window) return undefined
  try {
    return (window.parent as unknown as DemoHost).__cmdrDemoBackend
  } catch {
    // A cross-origin parent: someone else's page framing the demo.
    return undefined
  }
}

/** Installs this frame's role. Exported for its test; the app runs it only through the guard below. */
export function installDemoBackend(): void {
  const shared = parentBackend()
  if (shared) {
    installWebview(shared, window.name || 'secondary')
    return
  }
  const backend = createDemoBackend()
  ;(window as unknown as DemoHost).__cmdrDemoBackend = backend
  exposeUnanswered()
  installWebview(backend, 'main')
  // The macOS menu bar a page doesn't have, over the top frame's main window only: a secondary
  // window shares the one bar, as on a Mac.
  installMenuBar({
    menu: backend.menu,
    emit: (event, payload, target) => {
      backend.emit(event, payload, target)
    },
    windows: backend.windows,
  })
}

if (__CMDR_DEMO_BUILD__) installDemoBackend()
