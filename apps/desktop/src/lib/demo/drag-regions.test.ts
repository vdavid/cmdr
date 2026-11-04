import { afterEach, describe, expect, it, vi } from 'vitest'
import { installDragRegions } from './drag-regions.js'

let uninstall: (() => void) | undefined

function setup(markup: string) {
  document.body.innerHTML = markup
  const invoke = vi.fn()
  uninstall = installDragRegions(invoke)
  return invoke
}

function press(selector: string, init: MouseEventInit = {}) {
  const target = document.querySelector(selector)
  if (!target) throw new Error(`no ${selector}`)
  const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, detail: 1, ...init })
  target.dispatchEvent(down)
  return down
}

afterEach(() => {
  uninstall?.()
  uninstall = undefined
  document.body.replaceChildren()
})

// A port of Tauri's own `drag.js` (the script it injects into every webview), so the region rules
// are Tauri's: the app's `data-tauri-drag-region` strips behave as they do on desktop.
describe('drag regions', () => {
  it('starts a window drag from a press on a bare drag region, with where the press was', () => {
    const invoke = setup('<div id="strip" data-tauri-drag-region></div>')
    const down = press('#strip', { screenX: 40, screenY: 12 })
    expect(invoke).toHaveBeenCalledWith('plugin:window|start_dragging', { screenX: 40, screenY: 12 })
    expect(down.defaultPrevented).toBe(true)
  })

  it("follows Tauri's rules: a bare region is itself only, deep covers its subtree, a control blocks", () => {
    const invoke = setup(`
      <div id="bare" data-tauri-drag-region><span id="bare-child"></span></div>
      <div id="deep" data-tauri-drag-region="deep"><span id="deep-child"></span><button id="button"></button></div>
      <div id="off" data-tauri-drag-region="false"></div>
    `)
    press('#bare-child')
    press('#button')
    press('#off')
    press('#bare', { button: 2 })
    expect(invoke).not.toHaveBeenCalled()
    press('#deep-child')
    expect(invoke).toHaveBeenCalledOnce()
  })

  it('zooms on a double click released where it was pressed, as macOS does', () => {
    const invoke = setup('<div id="strip" data-tauri-drag-region></div>')
    press('#strip', { detail: 2, clientX: 5, clientY: 5 })
    expect(invoke).not.toHaveBeenCalled()
    const strip = document.querySelector('#strip')
    strip?.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, detail: 2, clientX: 5, clientY: 5 }))
    expect(invoke).toHaveBeenCalledWith('plugin:window|internal_toggle_maximize', {})
    press('#strip', { detail: 2, clientX: 5, clientY: 5 })
    strip?.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, detail: 2, clientX: 9, clientY: 5 }))
    expect(invoke).toHaveBeenCalledOnce()
  })
})
