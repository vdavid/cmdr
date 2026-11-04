import { afterEach, describe, expect, it, vi } from 'vitest'

// The engine's glue only exists after `pnpm build:listing-wasm`, so the test stands a fake in for
// it with the same surface (`listing-wasm.d.ts`).
const engine = vi.hoisted(() => {
  const state = {
    tree: undefined as unknown,
    onEvent: undefined as ((event: string, payload: unknown) => void) | undefined,
    invoked: [] as [string, unknown][],
    failInit: false,
  }
  return state
})

vi.mock('virtual:cmdr-listing-wasm', () => ({
  default: () => (engine.failInit ? Promise.reject(new Error('no wasm')) : Promise.resolve()),
  DemoListing: class {
    static handles(command: string): boolean {
      return command === 'list_directory_start_streaming'
    }
    constructor(tree: unknown, onEvent: (event: string, payload: unknown) => void) {
      engine.tree = tree
      engine.onEvent = onEvent
    }
    invoke(command: string, args: unknown): Promise<unknown> {
      engine.invoked.push([command, args])
      return Promise.resolve({ listingId: 'l1', status: 'loading' })
    }
  },
}))

import { DEMO_HOME, DEMO_ROOT, DEMO_VOLUME_ID } from './demo-tree.js'
import { _resetEngineModuleForTests } from './engine-module.js'
import { startListingEngine } from './listing.js'

afterEach(() => {
  _resetEngineModuleForTests()
  engine.invoked = []
  engine.failInit = false
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('listing engine', () => {
  it('builds the engine over the demo tree and routes what it handles', async () => {
    const handle = startListingEngine(() => undefined)
    expect(await handle.handles('list_directory_start_streaming')).toBe(true)
    expect(await handle.handles('get_volume_space')).toBe(false)
    expect(engine.tree).toMatchObject({ volumeId: DEMO_VOLUME_ID, root: DEMO_ROOT })
  })

  it('expands ~ in path and paths before the engine sees them', async () => {
    const handle = startListingEngine(() => undefined)
    await handle.invoke('list_directory_start_streaming', { path: '~/Documents/', paths: ['~', 7], sortBy: 'name' })
    expect(engine.invoked[0]).toEqual([
      'list_directory_start_streaming',
      { path: `${DEMO_HOME}/Documents`, paths: [DEMO_HOME, 7], sortBy: 'name' },
    ])
  })

  it('expands ~ in every write path argument, as the desktop does for root', async () => {
    const handle = startListingEngine(() => undefined)
    await handle.invoke('create_directory', { parentPath: '~', name: 'Work', volumeId: 'root' })
    await handle.invoke('copy_between_volumes', { sourcePaths: ['~/a.txt'], destPath: '~/Documents' })
    await handle.invoke('move_files', { sources: ['~/a.txt'], destination: '~/b' })
    await handle.invoke('rename_file', { from: '~/a.txt', to: '~/z.txt' })
    expect(engine.invoked.slice(-4)).toEqual([
      ['create_directory', { parentPath: DEMO_HOME, name: 'Work', volumeId: 'root' }],
      ['copy_between_volumes', { sourcePaths: [`${DEMO_HOME}/a.txt`], destPath: `${DEMO_HOME}/Documents` }],
      ['move_files', { sources: [`${DEMO_HOME}/a.txt`], destination: `${DEMO_HOME}/b` }],
      ['rename_file', { from: `${DEMO_HOME}/a.txt`, to: `${DEMO_HOME}/z.txt` }],
    ])
  })

  it('hands engine events on a task late, in order', async () => {
    vi.useFakeTimers()
    const heard: string[] = []
    const handle = startListingEngine((event) => heard.push(event))
    await handle.handles('x')
    engine.onEvent?.('listing-opening', { listingId: 'l1' })
    engine.onEvent?.('listing-complete', { listingId: 'l1' })
    // Not inside the call that caused them: the start command answers first.
    expect(heard).toEqual([])
    vi.runAllTimers()
    expect(heard).toEqual(['listing-opening', 'listing-complete'])
  })

  it('answers nothing, loudly once, when the engine fails to load', async () => {
    engine.failInit = true
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const handle = startListingEngine(() => undefined)
    expect(await handle.handles('list_directory_start_streaming')).toBe(false)
    await expect(handle.invoke('list_directory_start_streaming', {})).rejects.toThrow()
    expect(error).toHaveBeenCalledTimes(1)
  })
})
