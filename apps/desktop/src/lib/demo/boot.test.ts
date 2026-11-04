import { afterEach, describe, expect, it, vi } from 'vitest'

const calls = vi.hoisted(() => ({ created: 0, webviews: [] as [unknown, string][] }))

vi.mock('./backend.js', () => ({
  createDemoBackend: () => {
    calls.created++
    return { id: 'own backend' }
  },
}))
vi.mock('./webview.js', () => ({
  installWebview: (backend: unknown, label: string) => calls.webviews.push([backend, label]),
}))

// Imported for its export: under vitest `__CMDR_DEMO_BUILD__` is false, so the module installs nothing by itself.
import { installDemoBackend } from './boot.js'

type Host = { __cmdrDemoBackend?: unknown }

afterEach(() => {
  calls.created = 0
  calls.webviews = []
  delete (window as unknown as Host).__cmdrDemoBackend
  vi.restoreAllMocks()
})

describe('boot role', () => {
  it('installs nothing on import outside a demo build', () => {
    expect(calls.created).toBe(0)
    expect(calls.webviews).toEqual([])
  })

  it('makes the top frame the backend and its main webview', () => {
    installDemoBackend()
    expect(calls.created).toBe(1)
    expect(calls.webviews).toEqual([[{ id: 'own backend' }, 'main']])
    expect((window as unknown as Host).__cmdrDemoBackend).toEqual({ id: 'own backend' })
  })

  it("makes a demo window's iframe a webview over its parent's backend, labeled by its name", () => {
    const shared = { id: 'parent backend' }
    vi.spyOn(window, 'parent', 'get').mockReturnValue({ __cmdrDemoBackend: shared } as unknown as Window)
    vi.spyOn(window, 'name', 'get').mockReturnValue('settings')
    installDemoBackend()
    expect(calls.created).toBe(0)
    expect(calls.webviews).toEqual([[shared, 'settings']])
  })

  it('is its own top when framed by a page with no backend, or by another origin', () => {
    vi.spyOn(window, 'parent', 'get').mockReturnValue({} as Window)
    installDemoBackend()
    const crossOrigin = new Proxy(
      {},
      {
        get() {
          throw new DOMException('Blocked a frame from accessing a cross-origin frame', 'SecurityError')
        },
      },
    )
    vi.spyOn(window, 'parent', 'get').mockReturnValue(crossOrigin as Window)
    installDemoBackend()
    expect(calls.created).toBe(2)
    expect(calls.webviews.map(([, label]) => label)).toEqual(['main', 'main'])
  })
})
