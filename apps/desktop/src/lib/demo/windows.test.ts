import { afterEach, describe, expect, it } from 'vitest'
import type { EventTarget } from './backend.js'
import { createWindowHost } from './windows.js'

function setup() {
  const emitted: [string, EventTarget][] = []
  const host = createWindowHost((event, _payload, target) => emitted.push([event, target]))
  const call = (cmd: string, payload: Record<string, unknown> = {}): unknown => {
    const handler = host.handler(cmd)
    if (!handler) throw new Error(`no handler for ${cmd}`)
    return handler(payload)
  }
  const open = (label: string, extra: Record<string, unknown> = {}) =>
    call('plugin:webview|create_webview_window', {
      options: { label, url: '/settings?section=x', title: 'Settings', width: 900, height: 600, ...extra },
    })
  return { host, call, open, emitted }
}

const panels = () => [...document.querySelectorAll<HTMLElement>('.cmdr-demo-window')]

// happy-dom doesn't load iframe pages (each panel prints a "page loading is disabled" line from its
// own virtual console, which a spy can't reach); the iframe only has to exist with the right name
// and src here. The page loading in it is QA'd in a real browser (`DETAILS.md` § Secondary windows).
afterEach(() => {
  for (const panel of panels()) panel.remove()
})

describe('demo windows', () => {
  it('opens Settings as a panel holding an iframe of its route, named after its label', () => {
    const { open, call } = setup()
    open('settings')
    const [panel] = panels()
    const iframe = panel.querySelector('iframe')
    expect(iframe?.name).toBe('settings')
    expect(iframe?.getAttribute('src')).toBe('/settings?section=x')
    expect(panel.getAttribute('role')).toBe('dialog')
    expect(panel.querySelector('.cmdr-demo-window-title')?.textContent).toBe('Settings')
    expect(call('plugin:window|get_all_windows')).toEqual(['main', 'settings'])
    expect(call('plugin:webview|get_all_webviews')).toEqual([
      { label: 'main', windowLabel: 'main' },
      { label: 'settings', windowLabel: 'settings' },
    ])
  })

  it('fits the panel inside the viewport', () => {
    const { open } = setup()
    open('settings', { width: 5000, height: 5000 })
    const [panel] = panels()
    expect(parseFloat(panel.style.width)).toBeLessThanOrEqual(window.innerWidth)
    expect(parseFloat(panel.style.height)).toBeLessThanOrEqual(window.innerHeight)
  })

  it('refuses the windows the demo has no backend for, and a second window with the same label', () => {
    const { open } = setup()
    for (const label of ['queue', 'debug', 'viewer']) {
      expect(() => open(label)).toThrow()
    }
    expect(panels()).toHaveLength(0)
    open('shortcuts')
    expect(() => open('shortcuts')).toThrow()
    expect(panels()).toHaveLength(1)
  })

  it('opens several viewers by their `viewer-` labels, and each close frees that window', () => {
    const closed: string[] = []
    const host = createWindowHost(
      () => {},
      (label) => closed.push(label),
    )
    const call = (cmd: string, payload: Record<string, unknown>) => host.handler(cmd)?.(payload)
    for (const label of ['viewer-1', 'viewer-2']) {
      call('plugin:webview|create_webview_window', { options: { label, url: '/viewer?path=x', title: 'x' } })
    }
    expect(panels()).toHaveLength(2)
    call('close_child_window', { label: 'viewer-1' })
    call('plugin:window|destroy', { label: 'viewer-2' })
    expect(closed).toEqual(['viewer-1', 'viewer-2'])
  })

  it('closes by any of the app paths, gives the main window its focus back, and says so', () => {
    const { open, call, emitted } = setup()
    const button = document.createElement('button')
    document.body.append(button)
    button.focus()

    for (const cmd of ['plugin:window|close', 'plugin:window|destroy', 'close_child_window']) {
      open('settings')
      call('plugin:window|set_focus', { label: 'settings' })
      call(cmd, { label: 'settings' })
      expect(panels()).toHaveLength(0)
    }
    expect(document.activeElement).toBe(button)
    expect(emitted).toContainEqual(['tauri://destroyed', { kind: 'label', label: 'settings' }])
    expect(emitted.at(-1)).toEqual(['tauri://focus', { kind: 'label', label: 'main' }])
    button.remove()
  })

  it('closes from its own close button', () => {
    const { open } = setup()
    open('settings')
    panels()[0].querySelector<HTMLButtonElement>('.cmdr-demo-window-close')?.click()
    expect(panels()).toHaveLength(0)
  })

  it('raises a panel above the others and focuses it', () => {
    const { open, call, emitted } = setup()
    open('settings')
    open('shortcuts')
    call('raise_child_window', { label: 'settings' })
    const [settings, shortcuts] = panels()
    expect(settings.style.zIndex).toBe('var(--z-modal-top)')
    expect(shortcuts.style.zIndex).toBe('var(--z-modal)')
    expect(emitted.at(-1)).toEqual(['tauri://focus', { kind: 'label', label: 'settings' }])

    // Closing one hands focus to the other, as macOS does with the next window.
    call('plugin:window|close', { label: 'settings' })
    expect(emitted.at(-1)).toEqual(['tauri://focus', { kind: 'label', label: 'shortcuts' }])
  })

  it('focuses the window once its page loads, and on a click into it', () => {
    const { open, emitted } = setup()
    open('settings')
    const [panel] = panels()
    panel.querySelector('iframe')?.dispatchEvent(new Event('load'))
    expect(emitted.at(-1)).toEqual(['tauri://focus', { kind: 'label', label: 'settings' }])
    emitted.length = 0
    panel.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(emitted).toEqual([
      ['tauri://blur', { kind: 'label', label: 'main' }],
      ['tauri://focus', { kind: 'label', label: 'settings' }],
    ])
  })

  it('drags by its title bar, never past the viewport edge', () => {
    const { open } = setup()
    open('settings', { width: 400, height: 300 })
    const [panel] = panels()
    const bar = panel.querySelector<HTMLElement>('.cmdr-demo-window-bar')
    if (!bar) throw new Error('no title bar')
    bar.setPointerCapture = () => undefined
    const pointer = (type: string, x: number, y: number) =>
      bar.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, clientX: x, clientY: y, pointerId: 1 }))

    pointer('pointerdown', 10, 10)
    expect(panel.classList.contains('dragging')).toBe(true)
    pointer('pointermove', -500, -500)
    expect(panel.style.left).toBe('0px')
    expect(panel.style.top).toBe('0px')
    pointer('pointerup', 0, 0)
    expect(panel.classList.contains('dragging')).toBe(false)

    // A press on the close button isn't a drag.
    panel.querySelector('.cmdr-demo-window-close')?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(panel.classList.contains('dragging')).toBe(false)
  })

  it('treats window plumbing with no panel to act on as done', () => {
    const { call } = setup()
    expect(call('plugin:window|close', { label: 'nothing-open' })).toBeNull()
    expect(call('plugin:window|set_focus', { label: 'nothing-open' })).toBeNull()
    expect(call('raise_child_window', { label: 'nothing-open' })).toBeNull()
    expect(call('plugin:window|available_monitors')).toEqual([])
    expect(call('plugin:window|set_effects', { label: 'settings' })).toBeNull()
  })
})
