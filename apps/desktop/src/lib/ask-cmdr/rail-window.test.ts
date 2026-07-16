/**
 * The Tauri-touching side of rail-driven window growth: growing records how much it grew
 * and slid so the matching close reverses exactly that, a close with no record falls back
 * to removing one rail width (the persisted-open case), and a screen-filling window is left
 * alone. The pure geometry is covered in `window-positioning-utils.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { MonitorRect } from '$lib/window-positioning-utils'
import { growMainWindowForRail, shrinkMainWindowForRail } from './rail-window'

const win = {
  outerPosition: vi.fn<() => Promise<{ x: number; y: number }>>(),
  outerSize: vi.fn<() => Promise<{ width: number; height: number }>>(),
  scaleFactor: vi.fn<() => Promise<number>>(),
  isFullscreen: vi.fn<() => Promise<boolean>>(),
  isMaximized: vi.fn<() => Promise<boolean>>(),
  setSize: vi.fn<(s: unknown) => Promise<void>>(),
  setPosition: vi.fn<(p: unknown) => Promise<void>>(),
}
const readMonitorsMock = vi.fn<() => Promise<MonitorRect[]>>()

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }))
vi.mock('@tauri-apps/api/dpi', () => ({
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
  LogicalPosition: class {
    constructor(
      public x: number,
      public y: number,
    ) {}
  },
}))
vi.mock('$lib/window-positioning', () => ({ readMonitors: () => readMonitorsMock() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const MONITOR: MonitorRect = { x: 0, y: 0, width: 1920, height: 1080 }

/** Point the fake window at a logical-pixel rect (scale 1) and give its calls resolved values. */
function setWindow(
  rect: { x: number; y: number; width: number; height: number },
  { fullscreen = false, maximized = false } = {},
): void {
  win.outerPosition.mockResolvedValue({ x: rect.x, y: rect.y })
  win.outerSize.mockResolvedValue({ width: rect.width, height: rect.height })
  win.scaleFactor.mockResolvedValue(1)
  win.isFullscreen.mockResolvedValue(fullscreen)
  win.isMaximized.mockResolvedValue(maximized)
  win.setSize.mockResolvedValue(undefined)
  win.setPosition.mockResolvedValue(undefined)
}

beforeEach(async () => {
  vi.clearAllMocks()
  readMonitorsMock.mockResolvedValue([MONITOR])
  // Reset the module-level growth record to null: a screen-filling grow bails and clears it,
  // without touching setSize/setPosition — so each test starts with no recorded growth.
  setWindow({ x: 0, y: 0, width: 1920, height: 1080 }, { fullscreen: true })
  await growMainWindowForRail(0)
  vi.clearAllMocks()
  readMonitorsMock.mockResolvedValue([MONITOR])
})

describe('growMainWindowForRail', () => {
  it('grows rightward by the rail width, leaving the left edge put', async () => {
    setWindow({ x: 100, y: 100, width: 1080, height: 720 })
    await growMainWindowForRail(340)
    expect(win.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 100, y: 100 }))
    expect(win.setSize).toHaveBeenCalledWith(expect.objectContaining({ width: 1420, height: 720 }))
  })

  it('leaves a fullscreen window alone', async () => {
    setWindow({ x: 0, y: 0, width: 1920, height: 1080 }, { fullscreen: true })
    await growMainWindowForRail(340)
    expect(win.setSize).not.toHaveBeenCalled()
    expect(win.setPosition).not.toHaveBeenCalled()
  })

  it('leaves a maximized window alone', async () => {
    setWindow({ x: 0, y: 0, width: 1920, height: 1080 }, { maximized: true })
    await growMainWindowForRail(340)
    expect(win.setSize).not.toHaveBeenCalled()
  })
})

describe('shrinkMainWindowForRail', () => {
  it('reverses a preceding grow exactly, including the leftward slide', async () => {
    // Open near the right edge so growth has to slide the window left.
    setWindow({ x: 700, y: 100, width: 1080, height: 720 })
    await growMainWindowForRail(340)
    expect(win.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 500, y: 100 }))
    expect(win.setSize).toHaveBeenCalledWith(expect.objectContaining({ width: 1420 }))
    // The window now reports its grown geometry; closing must undo both the widen and the slide.
    setWindow({ x: 500, y: 100, width: 1420, height: 720 })
    await shrinkMainWindowForRail(340)
    expect(win.setSize).toHaveBeenLastCalledWith(expect.objectContaining({ width: 1080, height: 720 }))
    expect(win.setPosition).toHaveBeenLastCalledWith(expect.objectContaining({ x: 700, y: 100 }))
  })

  it('falls back to removing one rail width when nothing was recorded (persisted-open case)', async () => {
    setWindow({ x: 0, y: 0, width: 1420, height: 720 })
    await shrinkMainWindowForRail(340)
    expect(win.setSize).toHaveBeenCalledWith(expect.objectContaining({ width: 1080 }))
  })

  it('leaves a fullscreen window alone', async () => {
    setWindow({ x: 0, y: 0, width: 1920, height: 1080 }, { fullscreen: true })
    await shrinkMainWindowForRail(340)
    expect(win.setSize).not.toHaveBeenCalled()
  })
})
