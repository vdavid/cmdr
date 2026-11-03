/**
 * Tests for `entry-activation.ts`: what happens when a pane opens an entry
 * (Enter, ⌘↓, double-click, the Enter-behavior popup's choices). They pin the
 * arms that used to be reachable only through a mounted pane:
 * - a backend `redirectToPath` (git worktree / submodule) browses elsewhere,
 * - the archive / bundle Enter policy: `browse` falls through, `open` launches,
 *   `ask` raises the popup, and the whole policy is skipped once the PANE is
 *   already inside an archive,
 * - an archive file browses in place like a folder,
 * - a file inside an archive routes to the viewer, not the OS default app (what
 *   the default-app open says and refuses is `default-app-open.test.ts`'s),
 * - every search-results row leaves the snapshot volume first, and an
 *   unresolvable path navigates nowhere,
 * - going up remembers the folder we came from so it lands selected.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import { toCanonical, type CanonicalPath } from '#lib/path/canonical.js'
import type { FileEntry } from '../types'

const { launcher, settings, policy, viewer, navigate } = vi.hoisted<{
  launcher: { open: Mock<(path: string) => Promise<void>>; openOnVolume: Mock }
  settings: { getSetting: Mock }
  policy: {
    resolveEnterPolicy: Mock
    enterBehaviorFromSettings: Mock
    pathInsideArchive: Mock
    pathCrossesArchiveBoundary: Mock
  }
  viewer: { openFileViewer: Mock }
  navigate: { resolveLocationOrToast: Mock }
}>(() => ({
  // `open` sees the path alone, so most assertions stay one-argument; `openOnVolume`
  // is the module's real two-argument seam, for the test that pins the volume.
  launcher: { open: vi.fn(), openOnVolume: vi.fn() },
  settings: { getSetting: vi.fn() },
  policy: {
    resolveEnterPolicy: vi.fn(),
    enterBehaviorFromSettings: vi.fn(),
    // Two separate spies on purpose: this module asks the WIDE question about the
    // pane's path (is the pane already showing an archive's contents?) and the
    // NARROW one about an entry's path (is there a real file to hand the OS?).
    // One shared spy would let a site read the other's answer and still pass.
    pathInsideArchive: vi.fn(),
    pathCrossesArchiveBoundary: vi.fn(),
  },
  viewer: { openFileViewer: vi.fn() },
  navigate: { resolveLocationOrToast: vi.fn() },
}))

vi.mock('#lib/settings/index.js', () => ({ getSetting: settings.getSetting }))
vi.mock('./default-app-open', () => ({
  openWithDefaultAppOrExplain: (volumeId: string, entry: FileEntry) => {
    launcher.openOnVolume(volumeId, entry)
    return launcher.open(entry.path)
  },
}))
vi.mock('./archive-enter-policy', () => ({
  resolveEnterPolicy: policy.resolveEnterPolicy,
  enterBehaviorFromSettings: policy.enterBehaviorFromSettings,
}))
vi.mock('./archive-paths', () => ({
  pathInsideArchive: policy.pathInsideArchive,
  pathCrossesArchiveBoundary: policy.pathCrossesArchiveBoundary,
}))
vi.mock('#lib/file-viewer/open-viewer.js', () => ({ openFileViewer: viewer.openFileViewer }))
vi.mock('../navigation/navigate-and-select', () => ({ resolveLocationOrToast: navigate.resolveLocationOrToast }))

import { createEntryActivation, resolveActivation, type EntryActivationDeps } from './entry-activation'

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
    launcher.open.mockResolvedValue(undefined)
    policy.pathInsideArchive.mockReturnValue(false)
    policy.pathCrossesArchiveBoundary.mockReturnValue(false)
    policy.resolveEnterPolicy.mockReturnValue(null)
    policy.enterBehaviorFromSettings.mockReturnValue({})
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
      expect(launcher.open).not.toHaveBeenCalled()
    })

    it('launches the OS default app for `open`', async () => {
      policy.resolveEnterPolicy.mockReturnValue('open')
      await createEntryActivation(deps).handleNavigate(
        entryOf({ name: 'a.app', path: '/dir/a.app', isDirectory: true }),
      )
      expect(launcher.open).toHaveBeenCalledWith('/dir/a.app')
      expect(calls.loadDirectory).not.toHaveBeenCalled()
    })

    it('falls through to the browse arm for `browse`', async () => {
      policy.resolveEnterPolicy.mockReturnValue('browse')
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'a.zip', path: '/dir/a.zip', isArchive: true }))
      expect(calls.loadDirectory).toHaveBeenCalledWith({ path: '/dir/a.zip', selectName: undefined })
    })

    it('skips the policy entirely once the PANE is inside an archive', async () => {
      // The pane's path crosses the boundary; its entries are inner items. This
      // gate reads the WIDE predicate, because a pane sitting AT `/dir/a.zip` is
      // already showing the archive's contents.
      policy.pathCrossesArchiveBoundary.mockImplementation((p: string) => p.startsWith('/dir/a.zip'))
      // The entry itself is genuinely inside, so the viewer arm below takes the
      // narrow answer. Both are set here precisely because they differ: the pane
      // path is AT the archive, the entry path is IN it.
      policy.pathInsideArchive.mockImplementation((p: string) => p.startsWith('/dir/a.zip/'))
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

    it('stays put when the `..` row has no path to go to', async () => {
      // `FilePane` falls the row back to the empty path while the pane's own path
      // doesn't canonicalize, and the empty path means "this volume's root" to the
      // backend — so navigating it would leap past every level in between.
      paneState.currentPath = '/dir/sub'
      await createEntryActivation(deps).handleNavigate(entryOf({ name: '..', path: '', isDirectory: true }))
      expect(calls.loadDirectory).not.toHaveBeenCalled()
      expect(calls.setCurrentPath).not.toHaveBeenCalled()
    })

    it('routes a file inside an archive to the viewer with the pane drive volume', async () => {
      policy.pathInsideArchive.mockImplementation((p: string) => p.startsWith('/dir/a.zip/'))
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'inner.txt', path: '/dir/a.zip/inner.txt' }))
      expect(viewer.openFileViewer).toHaveBeenCalledWith('/dir/a.zip/inner.txt', 'root')
      expect(launcher.open).not.toHaveBeenCalled()
    })

    it('hands a plain file to the OS default app', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf())
      expect(launcher.open).toHaveBeenCalledWith('/dir/a.txt')
    })

    it('asks about the file on the pane’s own drive, which decides whether another app can open it', async () => {
      const entry = entryOf()
      await createEntryActivation(deps).handleNavigate(entry)
      expect(launcher.openOnVolume).toHaveBeenCalledExactlyOnceWith('root', entry)
    })

    it('launches nothing when the entry browses instead', async () => {
      await createEntryActivation(deps).handleNavigate(entryOf({ name: 'sub', path: '/dir/sub', isDirectory: true }))
      expect(launcher.openOnVolume).not.toHaveBeenCalled()
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
      expect(launcher.open).toHaveBeenCalledWith('/dir/a.zip')
    })
  })
})

describe('what a tool has to ask about', () => {
  const file = entryOf({ name: 'run.command', path: '/dir/run.command' })
  const folder = entryOf({ name: 'Photos', path: '/dir/Photos', isDirectory: true })
  const bundle = entryOf({ name: 'Thing.app', path: '/dir/Thing.app', isDirectory: true })

  function context(isSearchResultsView = false) {
    return { panePath: '/dir', isSearchResultsView, behavior: {} }
  }

  function activate() {
    const deps = {
      getCurrentPath: () => '/dir',
      setCurrentPath: vi.fn(),
      getCanonicalPath: () => null,
      getVolumeId: () => 'root',
      getIsSearchResultsView: () => false,
      loadDirectory: vi.fn().mockResolvedValue(undefined),
      openEnterMenu: vi.fn(),
      onGoToLocation: vi.fn(),
    }
    return { deps, activation: createEntryActivation(deps) }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    launcher.open.mockResolvedValue(undefined)
    policy.pathInsideArchive.mockReturnValue(false)
    policy.pathCrossesArchiveBoundary.mockReturnValue(false)
    policy.resolveEnterPolicy.mockReturnValue(null)
    policy.enterBehaviorFromSettings.mockReturnValue({})
  })

  describe('resolveActivation', () => {
    it('a plain file launches, and a folder browses', () => {
      expect(resolveActivation(file, context())).toBe('launch')
      expect(resolveActivation(folder, context())).toBe('browse')
    })

    it.each([
      ['browse', 'browse'],
      ['open', 'launch'],
      ['ask', 'choose'],
    ])('a bundle under the `%s` policy resolves to `%s`', (action, expected) => {
      policy.resolveEnterPolicy.mockReturnValue(action)
      expect(resolveActivation(bundle, context())).toBe(expected)
    })

    it('a document package set to Browse is still a file, so it launches: the policy alone never means safe', () => {
      policy.resolveEnterPolicy.mockReturnValue('browse')
      expect(resolveActivation(entryOf({ name: 'a.docx' }), context())).toBe('launch')
    })

    it('a file inside an archive goes to the viewer', () => {
      policy.pathInsideArchive.mockReturnValue(true)
      expect(resolveActivation(file, context())).toBe('viewer')
    })

    it('a redirect browses', () => {
      expect(resolveActivation(entryOf({ redirectToPath: '/elsewhere' }), context())).toBe('browse')
    })

    it('a search result under a policy leaves for its real volume, with no popup', () => {
      policy.resolveEnterPolicy.mockReturnValue('ask')
      expect(resolveActivation(bundle, context(true))).toBe('browse')
    })

    it('a plain file in search results still launches', () => {
      expect(resolveActivation(file, context(true))).toBe('launch')
    })
  })

  describe('openForTool', () => {
    it('a yes on what is still there opens it', async () => {
      const { activation } = activate()
      expect(await activation.openForTool(file, { expectedPath: file.path })).toBe('opened')
      expect(launcher.open).toHaveBeenCalledExactlyOnceWith(file.path)
    })

    it('a yes on something else opens nothing', async () => {
      const { activation } = activate()
      expect(await activation.openForTool(file, { expectedPath: '/dir/other' })).toBe('changedWhileAsking')
      expect(launcher.open).not.toHaveBeenCalled()
    })

    it('a yes on an entry set to Ask opens it, with no popup a stray Enter could answer', async () => {
      policy.resolveEnterPolicy.mockReturnValue('ask')
      const { deps, activation } = activate()
      expect(await activation.openForTool(bundle, { expectedPath: bundle.path })).toBe('opened')
      expect(deps.openEnterMenu).not.toHaveBeenCalled()
      expect(launcher.open).toHaveBeenCalledExactlyOnceWith(bundle.path)
    })

    it('with nobody asked, a folder browses', async () => {
      const { deps, activation } = activate()
      expect(await activation.openForTool(folder, { mayLaunch: false })).toBe('opened')
      expect(deps.loadDirectory).toHaveBeenCalled()
    })

    it('with nobody asked, a cursor that moved onto a file launches nothing', async () => {
      const { activation } = activate()
      expect(await activation.openForTool(file, { mayLaunch: false })).toBe('changedWhileAsking')
      expect(launcher.open).not.toHaveBeenCalled()
    })

    it('with nobody asked, an entry set to Ask raises no popup', async () => {
      policy.resolveEnterPolicy.mockReturnValue('ask')
      const { deps, activation } = activate()
      expect(await activation.openForTool(bundle, { mayLaunch: false })).toBe('changedWhileAsking')
      expect(deps.openEnterMenu).not.toHaveBeenCalled()
    })
  })
})
