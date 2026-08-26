/**
 * Tests for `entry-activation.ts`: what happens when a pane opens an entry
 * (Enter, ⌘↓, double-click, the Enter-behavior popup's choices). They pin the
 * arms that used to be reachable only through a mounted pane:
 * - a backend `redirectToPath` (git worktree / submodule) browses elsewhere,
 * - the archive / bundle Enter policy: `browse` falls through, `open` launches,
 *   `ask` raises the popup, and the whole policy is skipped once the PANE is
 *   already inside an archive,
 * - an archive file browses in place like a folder,
 * - a file inside an archive routes to the viewer, not the OS default app,
 * - every search-results row leaves the snapshot volume first, and an
 *   unresolvable path navigates nowhere,
 * - going up remembers the folder we came from so it lands selected.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import { toCanonical, type CanonicalPath } from '$lib/path/canonical'
import type { FileEntry } from '../types'

const { ipc, settings, policy, viewer, navigate } = vi.hoisted<{
  ipc: { openFile: Mock }
  settings: { getSetting: Mock }
  policy: { resolveEnterPolicy: Mock; parseEnterBehaviorOverrides: Mock; pathInsideArchive: Mock }
  viewer: { openFileViewer: Mock }
  navigate: { resolveLocationOrToast: Mock }
}>(() => ({
  ipc: { openFile: vi.fn() },
  settings: { getSetting: vi.fn() },
  policy: {
    resolveEnterPolicy: vi.fn(),
    parseEnterBehaviorOverrides: vi.fn(),
    pathInsideArchive: vi.fn(),
  },
  viewer: { openFileViewer: vi.fn() },
  navigate: { resolveLocationOrToast: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ openFile: ipc.openFile }))
vi.mock('$lib/settings', () => ({ getSetting: settings.getSetting }))
vi.mock('./archive-enter-policy', () => ({
  resolveEnterPolicy: policy.resolveEnterPolicy,
  parseEnterBehaviorOverrides: policy.parseEnterBehaviorOverrides,
}))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: policy.pathInsideArchive }))
vi.mock('$lib/file-viewer/open-viewer', () => ({ openFileViewer: viewer.openFileViewer }))
vi.mock('../navigation/navigate-and-select', () => ({ resolveLocationOrToast: navigate.resolveLocationOrToast }))

import { createEntryActivation, type EntryActivationDeps } from './entry-activation'

function entryOf(over: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'a.txt',
    path: '/dir/a.txt',
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
    ...over,
  }
}

const canonical = (path: string): CanonicalPath => toCanonical(path, '/Users/test')

describe('createEntryActivation', () => {
  let deps: EntryActivationDeps
  let calls: {
    setCurrentPath: Mock
    loadDirectory: Mock
    openEnterMenu: Mock
    onGoToLocation: Mock
  }
  let paneState: { currentPath: string; isSearchResultsView: boolean }

  beforeEach(() => {
    vi.clearAllMocks()
    ipc.openFile.mockResolvedValue(undefined)
    policy.pathInsideArchive.mockReturnValue(false)
    policy.resolveEnterPolicy.mockReturnValue(null)
    policy.parseEnterBehaviorOverrides.mockReturnValue({})
    settings.getSetting.mockReturnValue('')
    paneState = { currentPath: '/dir', isSearchResultsView: false }
    calls = {
      setCurrentPath: vi.fn((p: string) => {
        paneState.currentPath = p
      }),
      loadDirectory: vi.fn().mockResolvedValue(undefined),
      openEnterMenu: vi.fn(),
      onGoToLocation: vi.fn(),
    }
    deps = {
      getCurrentPath: () => paneState.currentPath,
      setCurrentPath: calls.setCurrentPath,
      getCanonicalPath: () => canonical(paneState.currentPath),
      getVolumeId: () => 'root',
      getIsSearchResultsView: () => paneState.isSearchResultsView,
      loadDirectory: calls.loadDirectory,
      openEnterMenu: calls.openEnterMenu,
      onGoToLocation: calls.onGoToLocation,
    }
  })

  describe('the redirect entries the backend marks', () => {
    it('browses to the redirect target', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'worktree', redirectToPath: '/elsewhere' }))
      expect(calls.setCurrentPath).toHaveBeenCalledWith('/elsewhere')
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/elsewhere' })
    })

    it('leaves the snapshot volume first on a search-results pane', async () => {
      paneState.isSearchResultsView = true
      navigate.resolveLocationOrToast.mockResolvedValue({ volumeId: 'root', path: '/elsewhere' })
      await createEntryActivation(deps).handleNavigate(entryOf({ redirectToPath: '/elsewhere' }))
      expect(calls.onGoToLocation).toHaveBeenCalledWith({ volumeId: 'root', path: '/elsewhere' })
      expect(calls.loadDirectory).not.toHaveBeenCalled()
    })
  })

  describe('the archive / bundle Enter policy', () => {
    it('raises the popup for `ask`', async () => {
      policy.resolveEnterPolicy.mockReturnValue('ask')
      const entry = entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true })
      await createEntryActivation(deps).handleNavigate(entry)
      expect(calls.openEnterMenu).toHaveBeenCalledWith(entry)
      expect(calls.loadDirectory).not.toHaveBeenCalled()
      expect(ipc.openFile).not.toHaveBeenCalled()
    })

    it('launches the OS default app for `open`', async () => {
      policy.resolveEnterPolicy.mockReturnValue('open')
      await createEntryActivation(deps).handleNavigate(
        entryOf({ name: 'a.app', path: '/dir/a.app', isDirectory: true }),
      )
      expect(ipc.openFile).toHaveBeenCalledWith('/dir/a.app')
      expect(calls.loadDirectory).not.toHaveBeenCalled()
    })

    it('falls through to the browse arm for `browse`', async () => {
      policy.resolveEnterPolicy.mockReturnValue('browse')
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true }))
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir/a.zip', selectName: undefined })
    })

    it('skips the policy entirely once the PANE is inside an archive', async () => {
      // The pane's path crosses the boundary; its entries are inner items.
      policy.pathInsideArchive.mockImplementation((p: string) => p.startsWith('/dir/a.zip'))
      paneState.currentPath = '/dir/a.zip'
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'inner.txt', path: '/dir/a.zip/inner.txt' }))
      expect(policy.resolveEnterPolicy).not.toHaveBeenCalled()
      expect(viewer.openFileViewer).toHaveBeenCalledWith('/dir/a.zip/inner.txt', 'root')
    })

    it('goes to the real volume instead of showing the popup on a search-results pane', async () => {
      policy.resolveEnterPolicy.mockReturnValue('ask')
      paneState.isSearchResultsView = true
      navigate.resolveLocationOrToast.mockResolvedValue({ volumeId: 'ext', path: '/dir/a.zip' })
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true }))
      expect(calls.openEnterMenu).not.toHaveBeenCalled()
      expect(calls.onGoToLocation).toHaveBeenCalledWith({ volumeId: 'ext', path: '/dir/a.zip' })
    })
  })

  describe('the ordinary arms', () => {
    it('browses into a directory', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'sub', path: '/dir/sub', isDirectory: true }))
      expect(calls.setCurrentPath).toHaveBeenCalledWith('/dir/sub')
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir/sub', selectName: undefined })
    })

    it('browses into an archive file in place', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true }))
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir/a.zip', selectName: undefined })
    })

    it('remembers the folder we came from when going up', async () => {
      paneState.currentPath = '/dir/sub'
      await createEntryActivation(deps).handleNavigate(entryOf({ name: '..', path: '/dir', isDirectory: true }))
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir', selectName: 'sub' })
    })

    it('routes a file inside an archive to the viewer with the pane drive volume', async () => {
      policy.pathInsideArchive.mockImplementation((p: string) => p.startsWith('/dir/a.zip/'))
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'inner.txt', path: '/dir/a.zip/inner.txt' }))
      expect(viewer.openFileViewer).toHaveBeenCalledWith('/dir/a.zip/inner.txt', 'root')
      expect(ipc.openFile).not.toHaveBeenCalled()
    })

    it('hands a plain file to the OS default app', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf())
      expect(ipc.openFile).toHaveBeenCalledWith('/dir/a.txt')
    })

    it('stays silent when the OS refuses to open a file', async () => {
      ipc.openFile.mockRejectedValue(new Error('no handler'))
      await expect(createEntryActivation(deps).handleNavigate(entryOf())).resolves.toBeUndefined()
    })

    it('navigates nowhere when a search-results row cannot be resolved', async () => {
      paneState.isSearchResultsView = true
      navigate.resolveLocationOrToast.mockResolvedValue(null)
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'sub', path: '/gone/sub', isDirectory: true }))
      expect(calls.onGoToLocation).not.toHaveBeenCalled()
      expect(calls.loadDirectory).not.toHaveBeenCalled()
    })
  })

  describe('the popup choices', () => {
    it('browse steps into the entry', async () => {
      await createEntryActivation(deps).browseIntoEntry(entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true }))
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir/a.zip', selectName: undefined })
    })

    it('open hands the entry to LaunchServices', async () => {
      await createEntryActivation(deps).openEntryExternally(entryOf({ name: 'a.zip', path: '/dir/a.zip' }))
      expect(ipc.openFile).toHaveBeenCalledWith('/dir/a.zip')
    })
  })
})
