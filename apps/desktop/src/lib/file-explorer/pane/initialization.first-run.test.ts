/**
 * Startup wiring for the one-shot first-run layout.
 *
 * `first-run-layout.test.ts` owns the rule; this file owns the two things only the call
 * site can get wrong: the chosen paths actually reaching the panes, and an E2E fixture
 * path still winning over them.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { getActiveTab } from '../tabs/tab-state-manager.svelte'
import type { PersistedPaneTabs } from '../tabs/tab-types'

const appStatus = vi.hoisted(() => ({
  loadAppStatus: vi.fn(),
  loadPaneTabs: vi.fn(),
  hasPersistedPaneState: vi.fn(),
  saveAppStatusNow: vi.fn(),
}))

const commands = vi.hoisted(() => ({
  pathExists: vi.fn(),
  getDefaultVolumeId: vi.fn(),
  resolvePathVolume: vi.fn(),
  getE2eStartPath: vi.fn(),
  checkFullDiskAccessQuiet: vi.fn(),
}))

const isE2eRun = vi.hoisted(() => vi.fn())

vi.mock('$lib/app-status-store', () => appStatus)
vi.mock('$lib/tauri-commands', () => commands)
vi.mock('$lib/app-mode', () => ({ isE2eRun }))
vi.mock('$lib/ask-cmdr/ask-cmdr-trigger.svelte', () => ({ hydrateRail: vi.fn() }))

/** What a never-launched install's store hands back: one home tab per pane, no marker. */
function freshPaneTabs(side: 'left' | 'right'): PersistedPaneTabs {
  return {
    tabs: [
      {
        id: `${side}-tab`,
        path: '~',
        volumeId: 'root',
        sortBy: 'name',
        sortOrder: 'ascending',
        viewMode: 'brief',
        pinned: false,
      },
    ],
    activeTabId: `${side}-tab`,
  }
}

/** A never-launched install's app status. */
const freshStatus = {
  leftPath: '~',
  rightPath: '~',
  focusedPane: 'left',
  leftViewMode: 'brief',
  rightViewMode: 'brief',
  leftVolumeId: 'root',
  rightVolumeId: 'root',
  leftSortBy: 'name',
  rightSortBy: 'name',
  leftPaneWidthPercent: 50,
  askCmdrRailOpen: false,
  askCmdrRailWidth: 340,
  firstRunLayoutApplied: false,
} as const

beforeEach(() => {
  vi.clearAllMocks()
  isE2eRun.mockReturnValue(false)
  appStatus.loadPaneTabs.mockImplementation((side: 'left' | 'right') => Promise.resolve(freshPaneTabs(side)))
  appStatus.loadAppStatus.mockResolvedValue(freshStatus)
  appStatus.hasPersistedPaneState.mockResolvedValue(false)
  appStatus.saveAppStatusNow.mockResolvedValue(undefined)
  commands.pathExists.mockResolvedValue(true)
  commands.getDefaultVolumeId.mockResolvedValue('root')
  commands.getE2eStartPath.mockResolvedValue(null)
  commands.checkFullDiskAccessQuiet.mockResolvedValue(true)
  commands.resolvePathVolume.mockResolvedValue({ volume: { id: 'root' }, timedOut: false })
})

async function initialize() {
  const { loadPersistedState } = await import('./initialization')
  return await loadPersistedState()
}

describe('loadPersistedState on a first run', () => {
  it('opens home on the left and Downloads on the right, and records the marker', async () => {
    const state = await initialize()
    expect(getActiveTab(state.leftTabMgr).path).toBe('~')
    expect(getActiveTab(state.rightTabMgr).path).toBe('~/Downloads')
    expect(appStatus.saveAppStatusNow).toHaveBeenCalledWith({ firstRunLayoutApplied: true })
  })

  it('leaves both panes on home when Full Disk Access is not granted', async () => {
    commands.checkFullDiskAccessQuiet.mockResolvedValue(false)
    const state = await initialize()
    expect(getActiveTab(state.leftTabMgr).path).toBe('~')
    expect(getActiveTab(state.rightTabMgr).path).toBe('~')
    expect(appStatus.saveAppStatusNow).not.toHaveBeenCalled()
  })

  it('keeps a returning user on their own paths, and marks the install so it stays that way', async () => {
    appStatus.hasPersistedPaneState.mockResolvedValue(true)
    appStatus.loadPaneTabs.mockImplementation((side: 'left' | 'right') => {
      const paneTabs = freshPaneTabs(side)
      paneTabs.tabs[0].path = `~/projects/${side}`
      return Promise.resolve(paneTabs)
    })

    const state = await initialize()

    expect(getActiveTab(state.leftTabMgr).path).toBe('~/projects/left')
    expect(getActiveTab(state.rightTabMgr).path).toBe('~/projects/right')
    expect(appStatus.saveAppStatusNow).toHaveBeenCalledWith({ firstRunLayoutApplied: true })
  })

  it('costs a marked install nothing on the way to the panes', async () => {
    // Startup latency: the common launch must not open the store or probe the permission
    // to re-derive an answer the marker already gave.
    appStatus.loadAppStatus.mockResolvedValue({ ...freshStatus, firstRunLayoutApplied: true })

    await initialize()

    expect(appStatus.hasPersistedPaneState).not.toHaveBeenCalled()
    expect(commands.checkFullDiskAccessQuiet).not.toHaveBeenCalled()
    expect(commands.pathExists).not.toHaveBeenCalledWith('~/Downloads')
  })

  it('lets an E2E fixture path win over the layout', async () => {
    isE2eRun.mockReturnValue(true)
    commands.getE2eStartPath.mockResolvedValue('/tmp/fixture')

    const state = await initialize()

    expect(getActiveTab(state.leftTabMgr).path).toBe('/tmp/fixture/left')
    expect(getActiveTab(state.rightTabMgr).path).toBe('/tmp/fixture/right')
    expect(appStatus.saveAppStatusNow).not.toHaveBeenCalled()
  })

  it('leaves the panes alone on an automated run with no fixture path', async () => {
    // The screenshot capture shard runs over the real home folder with no fixture path.
    // Moving its right pane into the real `~/Downloads` would rewrite every master.
    isE2eRun.mockReturnValue(true)

    const state = await initialize()

    expect(getActiveTab(state.leftTabMgr).path).toBe('~')
    expect(getActiveTab(state.rightTabMgr).path).toBe('~')
    expect(commands.pathExists).not.toHaveBeenCalled()
    expect(appStatus.saveAppStatusNow).not.toHaveBeenCalled()
  })
})
