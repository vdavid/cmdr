import { clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'
import { emit, emitTo, listen } from '@tauri-apps/api/event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { DemoBackend, DemoWebview } from './backend.js'

// `test-setup.ts` stubs the event API app-wide; this file tests the real one against the webview.
vi.unmock('@tauri-apps/api/event')
import { installWebview } from './webview.js'

/** A backend that records what reaches it and answers with what the test sets. */
function fakeBackend(answer: unknown = 'from backend') {
  const invoked: [string, unknown][] = []
  const emitted: [string, unknown, unknown][] = []
  let attached: DemoWebview | undefined
  const backend: Pick<DemoBackend, 'invoke' | 'emit' | 'attach'> = {
    invoke: (cmd, payload) => {
      invoked.push([cmd, payload])
      return Promise.resolve(answer)
    },
    emit: (event, payload, target) => emitted.push([event, payload, target]),
    attach: (webview) => {
      attached = webview
      return () => {
        attached = undefined
      }
    },
  }
  return { backend, invoked, emitted, webview: () => attached }
}

afterEach(() => {
  clearMocks()
})

describe('demo webview', () => {
  it('forwards a command to the backend over a JSON wire', async () => {
    const answer = { rows: [1, 2] }
    const fake = fakeBackend(answer)
    installWebview(fake.backend, 'main')
    const payload = { path: '/a', skipped: undefined }

    const result = await invoke<{ rows: number[] }>('get_thing', payload)
    expect(fake.invoked).toEqual([['get_thing', { path: '/a' }]])
    // A copy, so the app mutating an answer can't change the backend's state.
    expect(result).toEqual(answer)
    expect(result).not.toBe(answer)
  })

  it('answers questions about its own window itself, and forwards ones about another', async () => {
    const fake = fakeBackend()
    installWebview(fake.backend, 'main')
    expect(await invoke('plugin:window|scale_factor', { label: 'main' })).toBe(window.devicePixelRatio)
    expect(await invoke('plugin:window|outer_position', {})).toEqual({ x: 0, y: 0 })
    expect(await invoke('plugin:window|set_title', { label: 'main', value: 'x' })).toBeNull()
    expect(fake.invoked).toEqual([])

    await invoke('plugin:window|inner_size', { label: 'settings' })
    expect(fake.invoked).toEqual([['plugin:window|inner_size', { label: 'settings' }]])
  })

  it('sends emits to the backend, everyone or one label', async () => {
    const fake = fakeBackend()
    installWebview(fake.backend, 'main')
    await emit('settings-changed', { key: 'k' })
    await emitTo('settings', 'focus-search', null)
    expect(fake.emitted).toEqual([
      ['settings-changed', { key: 'k' }, { kind: 'everyone' }],
      ['focus-search', null, { kind: 'label', label: 'settings' }],
    ])
  })

  it('delivers backend events to its listeners, by target, until they unlisten', async () => {
    const fake = fakeBackend()
    installWebview(fake.backend, 'main')
    const any = vi.fn()
    const unlisten = await listen('ping', any)
    const onlySettings = vi.fn()
    await listen('ping', onlySettings, { target: { kind: 'Window', label: 'settings' } })

    const webview = fake.webview()
    webview?.deliver('ping', { n: 1 }, { kind: 'everyone' })
    webview?.deliver('ping', { n: 2 }, { kind: 'label', label: 'main' })
    webview?.deliver('other', {}, { kind: 'everyone' })

    expect(any.mock.calls.map(([event]) => (event as { payload: unknown }).payload)).toEqual([{ n: 1 }, { n: 2 }])
    // A label-targeted listener hears global events and its own label's, never another label's.
    expect(onlySettings.mock.calls.map(([event]) => (event as { payload: unknown }).payload)).toEqual([{ n: 1 }])

    unlisten()
    // The unlisten reaches the webview through an async `invoke`; let it land.
    await new Promise((resolve) => setTimeout(resolve, 0))
    webview?.deliver('ping', { n: 3 }, { kind: 'everyone' })
    expect(any).toHaveBeenCalledTimes(2)
  })

  it('detaches from the backend when its frame goes away', () => {
    const fake = fakeBackend()
    installWebview(fake.backend, 'settings')
    expect(fake.webview()?.label).toBe('settings')
    window.dispatchEvent(new Event('pagehide'))
    expect(fake.webview()).toBeUndefined()
  })
})
