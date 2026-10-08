/**
 * Startup wiring for a restored tab on an SMB share.
 *
 * ❗ A share that isn't mounted has no folder on disk, so the launch probe would walk
 * its path up to `/Volumes`, and the volume lookup would then file the tab under the
 * boot disk: the pin's whole point (the tab comes back to the share) lost. A SAVED
 * share keeps its id and its subfolder, and the pane's place-connect dials it; any
 * other share's tab walks up as before.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { getActiveTab } from '../tabs/tab-state-manager.svelte'
import type { PersistedPaneTabs } from '../tabs/tab-types'

const appStatus = vi.hoisted(() => ({
  loadAppStatus: vi.fn(),
  loadPaneTabs: vi.fn(),
  hasPersistedPaneState: vi.fn(),
  saveAppStatusNow: vi.fn(),
  savePaneTabs: vi.fn(),
  resolvePersistedPath: vi.fn(),
}))

const commands = vi.hoisted(() => ({
  pathExists: vi.fn(),
  getDefaultVolumeId: vi.fn(),
  resolvePathVolume: vi.fn(),
  getE2eStartPath: vi.fn(),
  checkFullDiskAccessQuiet: vi.fn(),
  listSavedServers: vi.fn(),
}))

vi.mock('$lib/app-status-store', () => appStatus)
vi.mock('$lib/tauri-commands', () => commands)
vi.mock('$lib/app-mode', () => ({ isE2eRun: () => false }))
vi.mock('$lib/ask-cmdr/ask-cmdr-trigger.svelte', () => ({ hydrateRail: vi.fn() }))

const shareId = 'smb-naspolya-445-naspi'
const existing = new Set(['/', '/Volumes', '~'])

function paneTabs(side: 'left' | 'right', path: string, volumeId: string): PersistedPaneTabs {
  return {
    tabs: [
      { id: `${side}-tab`, path, volumeId, sortBy: 'name', sortOrder: 'ascending', viewMode: 'brief', pinned: false },
    ],
    activeTabId: `${side}-tab`,
  }
}

/** The saved-server listing with one SMB host whose share `naspi` last mounted at `/Volumes/naspi`. */
function savedShare(connected: boolean) {
  return [
    {
      id: 'manual-naspolya',
      protocol: 'smb',
      places: [
        { volumeId: shareId, name: 'naspi', pinned: true, connected, appRoot: '/Volumes/naspi', username: null },
      ],
    },
  ]
}

beforeEach(() => {
  vi.clearAllMocks()
  appStatus.loadPaneTabs.mockImplementation((side: 'left' | 'right') =>
    Promise.resolve(
      side === 'left' ? paneTabs(side, '/Volumes/naspi/docs/2026', shareId) : paneTabs(side, '~', 'root'),
    ),
  )
  appStatus.loadAppStatus.mockResolvedValue({
    focusedPane: 'left',
    leftPaneWidthPercent: 50,
    askCmdrRailOpen: false,
    askCmdrRailWidth: 340,
    firstRunLayoutApplied: true,
  })
  appStatus.hasPersistedPaneState.mockResolvedValue(true)
  appStatus.saveAppStatusNow.mockResolvedValue(undefined)
  appStatus.savePaneTabs.mockResolvedValue(undefined)
  // The real walk, in miniature: the deepest ancestor that exists.
  appStatus.resolvePersistedPath.mockImplementation(async (path: string, exists: (p: string) => Promise<boolean>) => {
    let current = path
    while (!(await exists(current))) current = current.slice(0, current.lastIndexOf('/')) || '/'
    return current
  })
  commands.pathExists.mockImplementation((p: string) => Promise.resolve(existing.has(p)))
  commands.getDefaultVolumeId.mockResolvedValue('root')
  commands.getE2eStartPath.mockResolvedValue(null)
  commands.checkFullDiskAccessQuiet.mockResolvedValue(true)
  commands.resolvePathVolume.mockResolvedValue({ volume: { id: 'root' }, timedOut: false })
})

async function leftTab() {
  const { loadPersistedState } = await import('./initialization')
  const state = await loadPersistedState()
  return getActiveTab(state.leftTabMgr)
}

describe('loadPersistedState: a tab on an SMB share', () => {
  it('keeps an unmounted saved share and the folder inside it, for the pane to dial', async () => {
    commands.listSavedServers.mockResolvedValue(savedShare(false))

    const tab = await leftTab()

    expect(tab.volumeId).toBe(shareId)
    expect(tab.path).toBe('/Volumes/naspi/docs/2026')
  })

  it('walks a share nobody saved up to a folder that exists, on the volume that holds it', async () => {
    commands.listSavedServers.mockResolvedValue([])

    const tab = await leftTab()

    expect(tab.path).toBe('/Volumes')
    expect(tab.volumeId).toBe('root')
    expect(commands.resolvePathVolume).toHaveBeenCalledWith('/Volumes')
  })

  it('probes a mounted share like any folder, so a folder deleted inside it still walks up', async () => {
    commands.listSavedServers.mockResolvedValue(savedShare(true))
    existing.add('/Volumes/naspi')
    existing.add('/Volumes/naspi/docs')
    commands.resolvePathVolume.mockResolvedValue({ volume: { id: shareId }, timedOut: false })
    try {
      const tab = await leftTab()

      expect(tab.path).toBe('/Volumes/naspi/docs')
      expect(tab.volumeId).toBe(shareId)
    } finally {
      existing.delete('/Volumes/naspi')
      existing.delete('/Volumes/naspi/docs')
    }
  })

  it('asks for the saved servers only when a tab stands on a share', async () => {
    appStatus.loadPaneTabs.mockImplementation((side: 'left' | 'right') => Promise.resolve(paneTabs(side, '~', 'root')))

    await leftTab()

    expect(commands.listSavedServers).not.toHaveBeenCalled()
  })
})
