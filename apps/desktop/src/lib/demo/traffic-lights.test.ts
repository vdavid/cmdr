import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMainWindowLights, createTrafficLights, lightsInset, MAIN_LIGHTS_INSET } from './traffic-lights.js'

const light = (root: HTMLElement, kind: string) => root.querySelector<HTMLButtonElement>(`[data-light="${kind}"]`)

function stubFullScreen(enabled: boolean) {
  const request = vi.fn(() => Promise.resolve())
  Object.defineProperty(document, 'fullscreenEnabled', { configurable: true, value: enabled })
  Object.defineProperty(document.documentElement, 'requestFullscreen', { configurable: true, value: request })
  return request
}

afterEach(() => {
  Reflect.deleteProperty(document, 'fullscreenEnabled')
  Reflect.deleteProperty(document.documentElement, 'requestFullscreen')
})

describe('traffic lights', () => {
  it("places each window's lights from its trafficLightPosition, the main window's from tauri.conf.json", () => {
    // Measured on macOS 27 off the real windows: (9, 17) draws at (9, 8), (20, 29) at (20, 20).
    expect(lightsInset({ x: 9, y: 17 })).toEqual({ left: 9, top: 8 })
    expect(lightsInset({ x: 20, y: 29 })).toEqual({ left: 20, top: 20 })
    expect(MAIN_LIGHTS_INSET).toEqual({ left: 9, top: 8 })
  })

  it('draws close, minimize, and zoom in macOS order, labeled, out of the Tab order', () => {
    const { root } = createTrafficLights({}, 'zoom')
    const buttons = [...root.querySelectorAll<HTMLButtonElement>('button')]
    expect(buttons.map((b) => b.dataset.light)).toEqual(['close', 'minimize', 'zoom'])
    for (const button of buttons) {
      expect(button.getAttribute('aria-label')).toBeTruthy()
      expect(button.tabIndex).toBe(-1)
    }
    expect(document.getElementById('cmdr-demo-lights-style')).not.toBeNull()
  })

  it('runs the actions it has and marks the others inert', () => {
    const close = vi.fn()
    const { root } = createTrafficLights({ close }, 'zoom')
    light(root, 'close')?.click()
    expect(close).toHaveBeenCalledOnce()
    expect(light(root, 'close')?.hasAttribute('aria-disabled')).toBe(false)
    expect(light(root, 'zoom')?.getAttribute('aria-disabled')).toBe('true')
  })

  it('goes grey when its window is not key', () => {
    const lights = createTrafficLights({}, 'zoom')
    expect(lights.root.dataset.active).toBe('true')
    lights.setActive(false)
    expect(lights.root.dataset.active).toBe('false')
  })

  it("makes the main window's green light the browser's full screen, when the page may have it", () => {
    const request = stubFullScreen(true)
    const { root } = createMainWindowLights()
    expect(root.dataset.zoom).toBe('enterFullScreen')
    light(root, 'zoom')?.click()
    expect(request).toHaveBeenCalledOnce()
    // A page can't close or hide its own tab.
    expect(light(root, 'close')?.getAttribute('aria-disabled')).toBe('true')
    expect(light(root, 'minimize')?.getAttribute('aria-disabled')).toBe('true')
  })

  it('leaves the green light inert in a frame that may not go full screen', () => {
    const request = stubFullScreen(false)
    const { root } = createMainWindowLights()
    light(root, 'zoom')?.click()
    expect(request).not.toHaveBeenCalled()
    expect(light(root, 'zoom')?.getAttribute('aria-disabled')).toBe('true')
  })
})
