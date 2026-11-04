import { describe, expect, it, vi } from 'vitest'

// A fake engine: claims two commands, one of which a TS table also answers, to pin the order.
const engine = vi.hoisted(() => ({
  sink: undefined as ((event: string, payload: unknown, target?: string) => void) | undefined,
  calls: [] as string[],
  closedWindows: [] as string[],
}))

vi.mock('./listing.js', () => ({
  startListingEngine: (sink: (event: string, payload: unknown, target?: string) => void) => {
    engine.sink = sink
    return {
      handles: (cmd: string) => Promise.resolve(cmd === 'get_file_range' || cmd === 'get_default_volume_id'),
      invoke: (cmd: string) => {
        engine.calls.push(cmd)
        return Promise.resolve(`engine:${cmd}`)
      },
      closeViewerWindow: (label: string) => engine.closedWindows.push(label),
    }
  },
}))

import { createDemoBackend, type DemoWebview, type EventTarget } from './backend.js'
import { DEMO_VOLUME_ID } from './demo-tree.js'

function fakeWebview(label: string) {
  const heard: [string, unknown, EventTarget][] = []
  const webview: DemoWebview = { label, deliver: (event, payload, target) => heard.push([event, payload, target]) }
  return { webview, heard }
}

describe('demo backend', () => {
  it('answers from its TS tables before asking the engine', async () => {
    const backend = createDemoBackend()
    // `get_default_volume_id` is claimed by both: the TS answer wins.
    expect(await backend.invoke('get_default_volume_id', {})).toBe(DEMO_VOLUME_ID)
    expect(engine.calls).not.toContain('get_default_volume_id')
  })

  it('routes a command only the engine handles to the engine', async () => {
    const backend = createDemoBackend()
    expect(await backend.invoke('get_file_range', { listingId: 'l' })).toBe('engine:get_file_range')
  })

  it('answers plugin, raw, silent, and window commands', async () => {
    const backend = createDemoBackend()
    expect(await backend.invoke('plugin:app|name', {})).toBe('Cmdr')
    expect(await backend.invoke('configure_ai', {})).toEqual({ secretStoreError: null })
    expect(await backend.invoke('track_event', { name: 'x', propsJson: '{}' })).toBeUndefined()
    expect(await backend.invoke('plugin:window|get_all_windows', {})).toEqual(['main'])
  })

  it('notes a command nobody answers and answers null', async () => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    const backend = createDemoBackend()
    expect(await backend.invoke('no_such_command', {})).toBeNull()
  })

  it('fans every event out to every attached webview, engine events included', () => {
    const backend = createDemoBackend()
    const main = fakeWebview('main')
    const settings = fakeWebview('settings')
    backend.attach(main.webview)
    const detach = backend.attach(settings.webview)

    engine.sink?.('listing-complete', { listingId: 'l' })
    backend.emit('settings-changed', { key: 'k' }, { kind: 'label', label: 'settings' })
    expect(main.heard.map(([event]) => event)).toEqual(['listing-complete', 'settings-changed'])
    expect(settings.heard[1]).toEqual(['settings-changed', { key: 'k' }, { kind: 'label', label: 'settings' }])

    detach()
    backend.emit('later', null, { kind: 'everyone' })
    expect(settings.heard).toHaveLength(2)
    expect(main.heard).toHaveLength(3)
  })

  it('sends an engine event with a target to that window only, and keeps its own notices', () => {
    const backend = createDemoBackend()
    const viewer = fakeWebview('viewer-1')
    backend.attach(viewer.webview)
    const opened = vi.spyOn(window, 'open').mockImplementation(() => null)

    engine.sink?.('viewer-media-load', { token: 't' }, 'viewer-1')
    engine.sink?.('demo://open-url', { url: 'https://getcmdr.com/' })
    engine.sink?.('demo://media-token-dropped', { token: 't' })
    expect(viewer.heard).toEqual([['viewer-media-load', { token: 't' }, { kind: 'label', label: 'viewer-1' }]])
    expect(opened).toHaveBeenCalledWith('https://getcmdr.com/', '_blank', 'noopener')
    opened.mockRestore()
  })

  it("frees a viewer window's sessions when its panel closes", async () => {
    const backend = createDemoBackend()
    await backend.invoke('plugin:webview|create_webview_window', {
      options: { label: 'viewer-9', url: '/viewer?path=x', title: 'x' },
    })
    await backend.invoke('close_child_window', { label: 'viewer-9' })
    expect(engine.closedWindows).toContain('viewer-9')
    document.querySelector('.cmdr-demo-window')?.remove()
  })
})
