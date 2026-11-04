import { describe, expect, it, vi } from 'vitest'

// A fake engine: claims two commands, one of which a TS table also answers, to pin the order.
const engine = vi.hoisted(() => ({
  sink: undefined as ((event: { name: string; payload: unknown; target?: string }) => void) | undefined,
  calls: [] as string[],
  closedWindows: [] as string[],
}))

vi.mock('./listing.js', () => ({
  startListingEngine: (sink: (event: { name: string; payload: unknown; target?: string }) => void) => {
    engine.sink = sink
    return {
      handles: (cmd: string) => Promise.resolve(cmd === 'get_file_range' || cmd === 'get_default_volume_id'),
      invoke: (cmd: string) => {
        engine.calls.push(cmd)
        return Promise.resolve(`engine:${cmd}`)
      },
      closeViewerWindow: (label: string) => engine.closedWindows.push(label),
      mediaBody: (token: string) =>
        token === 'known' ? { mime: 'image/png', bytes: Uint8Array.from([1, 2, 3]) } : undefined,
    }
  },
}))

// A fake menu engine claiming one push, so the test pins that menu pushes reach it, not a TS table.
vi.mock('./menu-engine.js', () => ({
  startMenuEngine: () => ({
    ready: Promise.resolve(true),
    handles: (cmd: string) => Promise.resolve(cmd === 'activate_window_menu'),
    invoke: (cmd: string) => {
      engine.calls.push(`menu:${cmd}`)
      return Promise.resolve(null)
    },
    bar: () => [],
    click: () => ({ kind: 'nothing' }),
  }),
}))

import { createDemoBackend, type DemoWebview, type EventTarget } from './backend.js'
import { DEMO_HOME, DEMO_VOLUME_ID } from './demo-tree.js'

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

  it('hands the menu-state pushes to the menu engine', async () => {
    const backend = createDemoBackend()
    expect(await backend.invoke('activate_window_menu', { kind: 'main' })).toBeNull()
    expect(engine.calls).toContain('menu:activate_window_menu')
  })

  it('answers plugin, raw, silent, and window commands', async () => {
    const backend = createDemoBackend()
    expect(await backend.invoke('plugin:app|name', {})).toBe('Cmdr')
    expect(await backend.invoke('configure_ai', {})).toEqual({ secretStoreError: null })
    expect(await backend.invoke('track_event', { name: 'x', propsJson: '{}' })).toBeUndefined()
    expect(await backend.invoke('plugin:window|get_all_windows', {})).toEqual(['main'])
  })

  it('resolves an unanswered void command and rejects any other, as Tauri does', async () => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    const backend = createDemoBackend()
    expect(await backend.invoke('fill_git_commit_counts', {})).toBeNull()
    await expect(backend.invoke('no_such_command', {})).rejects.toMatch(/no_such_command/)
  })

  it('fans every event out to every attached webview, engine events included', () => {
    const backend = createDemoBackend()
    const main = fakeWebview('main')
    const settings = fakeWebview('settings')
    backend.attach(main.webview)
    const detach = backend.attach(settings.webview)

    engine.sink?.({ name: 'listing-complete', payload: { listingId: 'l' } })
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

    engine.sink?.({ name: 'viewer-media-load', payload: { token: 't' }, target: 'viewer-1' })
    engine.sink?.({ name: 'demo://open-url', payload: { url: 'https://getcmdr.com/' } })
    // The shared opener's scheme check holds for a viewer link too.
    const warned = vi.spyOn(console, 'warn').mockImplementation(() => {})
    engine.sink?.({ name: 'demo://open-url', payload: { url: 'javascript:alert(1)' } })
    warned.mockRestore()
    engine.sink?.({ name: 'demo://media-token-dropped', payload: { token: 't' } })
    expect(viewer.heard).toEqual([['viewer-media-load', { token: 't' }, { kind: 'label', label: 'viewer-1' }]])
    expect(opened).toHaveBeenCalledTimes(1)
    expect(opened).toHaveBeenCalledWith('https://getcmdr.com/', '_blank', 'noopener,noreferrer')
    opened.mockRestore()
  })

  it('makes one blob: URL per media token and revokes it when the engine drops the token', () => {
    const backend = createDemoBackend()
    const created = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:one')
    const revoked = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)

    expect(backend.mediaUrl('known', 'viewer-1')).toBe('blob:one')
    expect(backend.mediaUrl('known', 'viewer-1')).toBe('blob:one')
    expect(backend.mediaUrl('unknown', 'viewer-1')).toBe('')
    expect(created).toHaveBeenCalledTimes(1)

    engine.sink?.({ name: 'demo://media-token-dropped', payload: { token: 'known' } })
    engine.sink?.({ name: 'demo://media-token-dropped', payload: { token: 'never-made' } })
    expect(revoked.mock.calls).toEqual([['blob:one']])
    created.mockRestore()
    revoked.mockRestore()
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

  it('opens a demo file in the viewer on Enter, and keeps the refusal for a folder', async () => {
    const backend = createDemoBackend()
    const main = fakeWebview('main')
    backend.attach(main.webview)

    expect(await backend.invoke('open_path', { path: '~/notes.md', volumeId: 'root' })).toBeNull()
    expect(main.heard).toEqual([
      ['open-file-viewer', { path: `${DEMO_HOME}/notes.md` }, { kind: 'label', label: 'main' }],
    ])
    await expect(backend.invoke('open_path', { path: `${DEMO_HOME}/Music`, volumeId: 'root' })).rejects.toMatchObject({
      type: 'unsupported',
    })
  })
})
