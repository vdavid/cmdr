/**
 * Vitest test setup file.
 * Polyfills browser APIs not available in jsdom.
 */

import { mockConvertFileSrc } from '@tauri-apps/api/mocks'
import { afterEach, beforeEach, vi } from 'vitest'
import { TestResizeObserver } from '#lib/test-layout.js'
import { _forgetStartupCatalogForTests } from '#lib/intl/startup-catalog.js'

// The message runtime starts in the language the last switch landed on, remembered in
// localStorage, which Vitest keeps across files and runs. Forget it BEFORE the runtime
// first loads (hence the dynamic import below), and before every test, so each runtime,
// a `vi.resetModules()` one included, starts on the default language.
_forgetStartupCatalogForTests()
beforeEach(() => {
  _forgetStartupCatalogForTests()
  // `mediaUrl` builds through Tauri's `convertFileSrc`, which reads the webview's internals:
  // answer it as macOS's webview does, so a media URL reads `cmdr-media://localhost/<token>`.
  mockConvertFileSrc('macos')
})

// The app loads a window's catalogs on demand, which makes `setLocale()` async. Every
// catalog here, kept for the whole file, keeps it synchronous in tests, so a test can
// `setLocale('de')` and assert on `t()` on the next line. `lazy-catalogs.svelte.test.ts`
// covers the on-demand behavior itself.
const { _loadAllCatalogsForTests, installMessageRuntimeHooks } = await import('#lib/intl/messages.svelte.js')
await _loadAllCatalogsForTests()
// Test doubles for what the app's boot code (`routes/+layout.ts`) wires in. ❌ Never import
// the real logger or settings store into this file: every module this file reaches selects
// the WHOLE suite in `svelte-tests-related` (`test-setup-closure.test.ts`).
installMessageRuntimeHooks({ warn: () => {}, setTextSize: () => {} })

// Every component a test mounts is unmounted once that test ends, whether or not the
// test does it. A component left mounted keeps its timers, effects, and frame loops
// running past its test and past the file, after the DOM environment has removed its
// globals: FilePane's status announcer, firing 750 ms on, asked for a frame from a dead
// environment (`ReferenceError: requestAnimationFrame is not defined`, an unhandled
// error that failed CI at random). Unmounting runs each component's own teardown, which
// cancels that work.
//
// `unmount` is wrapped too. A component this cleanup took down is a no-op to unmount
// again, so a test that clears its leftovers at the START of the next test (several do)
// doesn't trip Svelte's double-unmount warning; any other double unmount still warns.
const componentLifetimes = vi.hoisted(() => ({
  mounted: new Set<Record<string, unknown>>(),
  cleanedUp: new WeakSet<Record<string, unknown>>(),
  unmount: null as null | ((component: Record<string, unknown>) => Promise<void>),
}))
vi.mock('svelte', async (importOriginal) => {
  const svelte = await importOriginal<typeof import('svelte')>()
  componentLifetimes.unmount = svelte.unmount
  const mount = ((component, options) => {
    const instance = svelte.mount(component, options)
    componentLifetimes.mounted.add(instance)
    return instance
  }) as typeof svelte.mount
  const unmount = ((component, options) => {
    if (componentLifetimes.cleanedUp.has(component)) return Promise.resolve()
    componentLifetimes.mounted.delete(component)
    return svelte.unmount(component, options)
  }) as typeof svelte.unmount
  return { ...svelte, mount, unmount }
})
afterEach(async () => {
  const { mounted, cleanedUp, unmount } = componentLifetimes
  // Re-check membership: unmounting a parent also unmounts the roots it mounted itself
  // (an Ark `Portal`'s content), which drops them from the set mid-loop.
  for (const component of [...mounted]) {
    if (!mounted.delete(component)) continue
    cleanedUp.add(component)
    await unmount?.(component)
  }
})

// ResizeObserver is not available in the test DOM. This one lets components
// that use it mount, and stays silent until a test drives a resize through
// `installLayoutMock().resize()` (`#lib/test-layout`) — there is no layout
// engine here, so nothing else could ever make it fire.
vi.stubGlobal('ResizeObserver', TestResizeObserver)

// Mock Tauri event API to handle both static and dynamic imports
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}))

// Mock Tauri path API (used by FilePane for ~ substitution in breadcrumbs)
vi.mock('@tauri-apps/api/path', () => ({
  homeDir: vi.fn(() => Promise.resolve('/Users/test')),
}))

// Mock Tauri webview API for drag-and-drop
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: vi.fn(() => ({
    onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
  })),
}))

// Stub canvas 2D contexts for jsdom. Production code uses canvas in five
// spots: font width measurement (`font-metrics/`, `full-list-utils.ts`),
// text layout (`@chenglou/pretext` via `shorten-middle.ts`), drag preview
// rendering (`drag-image-renderer.ts`), and viewer line-height calc
// (`viewer-line-heights.svelte.ts`). jsdom doesn't implement the Canvas API;
// without this stub it prints "Not implemented: HTMLCanvasElement's
// getContext()" once per call, drowning real warnings in the noise.
// Pixel-accurate output isn't meaningful in jsdom (WebKit vs Cairo render
// differently) — visual regression belongs in Playwright. So `measureText`
// returns a synthetic width (7 px per character) and every draw method is a
// no-op. The faked width is good enough for wrapper logic to thread non-zero
// numbers through; tests that need real font metrics don't exist and would
// have to use Playwright anyway.
function createMockCanvas2DContext(): CanvasRenderingContext2D {
  const handler: ProxyHandler<CanvasRenderingContext2D> = {
    get(_target, prop) {
      if (prop === 'measureText') {
        return (text: string): TextMetrics =>
          ({
            width: text.length * 7,
            actualBoundingBoxAscent: 10,
            actualBoundingBoxDescent: 3,
            actualBoundingBoxLeft: 0,
            actualBoundingBoxRight: text.length * 7,
            fontBoundingBoxAscent: 11,
            fontBoundingBoxDescent: 4,
          }) as TextMetrics
      }
      // Every other method/property: no-op function. Property reads (font,
      // fillStyle, etc.) get a callable too — harmless since callers only
      // assign to those, never read them back.
      return () => undefined
    },
    set() {
      return true // Accept any property write (font, fillStyle, lineWidth, …).
    },
  }
  return new Proxy({} as CanvasRenderingContext2D, handler)
}

HTMLCanvasElement.prototype.getContext = vi.fn((contextId: string) => {
  return contextId === '2d' ? createMockCanvas2DContext() : null
}) as typeof HTMLCanvasElement.prototype.getContext

// jsdom 29.1+ defines `OffscreenCanvas` as a global, but its 2D context is
// null (no rendering backend). Text-measurement code prefers OffscreenCanvas
// when present (`@chenglou/pretext`'s `getMeasureContext`), so without this it
// hits a null context and throws on `ctx.font = …`. Route it to the same
// synthetic 2D context as the DOM canvas above.
class OffscreenCanvasMock {
  constructor(
    public width: number,
    public height: number,
  ) {}

  getContext(contextId: string): CanvasRenderingContext2D | null {
    return contextId === '2d' ? createMockCanvas2DContext() : null
  }
}

vi.stubGlobal('OffscreenCanvas', OffscreenCanvasMock)
