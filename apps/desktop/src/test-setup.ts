/**
 * Vitest test setup file.
 * Polyfills browser APIs not available in jsdom.
 */

import { vi } from 'vitest'

// ResizeObserver is not available in jsdom
// This mock allows components that use ResizeObserver to run in tests
class ResizeObserverMock {
  callback: ResizeObserverCallback

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback
  }

  observe() {
    // No-op in tests
  }

  unobserve() {
    // No-op in tests
  }

  disconnect() {
    // No-op in tests
  }
}

vi.stubGlobal('ResizeObserver', ResizeObserverMock)

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
