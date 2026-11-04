import { afterEach, describe, expect, it, vi } from 'vitest'

// The engine's glue only exists after `pnpm build:listing-wasm`, so the test stands a fake in for
// it with the same surface (`listing-wasm.d.ts`).
const engine = vi.hoisted(() => ({
  invoked: [] as [string, unknown][],
  failInit: false,
}))

vi.mock('virtual:cmdr-listing-wasm', () => ({
  default: () => (engine.failInit ? Promise.reject(new Error('no wasm')) : Promise.resolve()),
  DemoMenuBar: class {
    static handles(command: string): boolean {
      return command === 'activate_window_menu'
    }
    invoke(command: string, args: unknown): unknown {
      engine.invoked.push([command, args])
      return null
    }
    bar(): unknown {
      return [{ id: 'app', title: { text: 'Cmdr' }, rows: [{ kind: 'separator' }] }]
    }
    click(menuId: string): unknown {
      return { kind: 'emit', event: 'execute-command', payload: { commandId: menuId } }
    }
  },
}))

import { _resetEngineModuleForTests } from './engine-module.js'
import { startMenuEngine } from './menu-engine.js'

afterEach(() => {
  _resetEngineModuleForTests()
  engine.invoked = []
  engine.failInit = false
  vi.restoreAllMocks()
})

describe('menu engine', () => {
  it('routes the pushes the engine claims and answers the bar and a click from it', async () => {
    const handle = startMenuEngine()
    expect(await handle.ready).toBe(true)
    expect(await handle.handles('activate_window_menu')).toBe(true)
    expect(await handle.handles('get_volume_space')).toBe(false)

    await handle.invoke('activate_window_menu', { label: 'main' })
    expect(engine.invoked).toEqual([['activate_window_menu', { label: 'main' }]])
    expect(handle.bar()).toEqual([{ id: 'app', title: { text: 'Cmdr' }, rows: [{ kind: 'separator' }] }])
    expect(handle.click('view.palette')).toEqual({
      kind: 'emit',
      event: 'execute-command',
      payload: { commandId: 'view.palette' },
    })
  })

  it('draws nothing, claims nothing, and refuses a push when the engine fails to load', async () => {
    engine.failInit = true
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    const handle = startMenuEngine()
    expect(await handle.ready).toBe(false)
    expect(await handle.handles('activate_window_menu')).toBe(false)
    expect(handle.bar()).toBeNull()
    expect(handle.click('view.palette')).toEqual({ kind: 'nothing' })
    await expect(handle.invoke('activate_window_menu', {})).rejects.toThrow(/did not load/)
  })
})
