import { describe, it, expect, vi, beforeEach } from 'vitest'

// Hoisted mocks: vi.mock factories cannot reference top-level bindings, so we
// declare the spy fns inside vi.hoisted() and reuse them from the mock
// factories AND from the test body.
const {
  resolveGoToPathMock,
  resolveLocationOrToastMock,
  addRecentPathStateMock,
  addToastMock,
  getEffectiveShortcutsMock,
  navigateToDirMock,
  navigateToFileMock,
  getFocusedPaneMock,
  getFocusedPanePathMock,
  readSchemeInputMock,
  actOnSchemeInputMock,
} = vi.hoisted(() => ({
  readSchemeInputMock: vi.fn<() => Promise<unknown>>(() => Promise.resolve(null)),
  actOnSchemeInputMock: vi.fn<() => Promise<unknown>>(),
  resolveGoToPathMock: vi.fn(),
  resolveLocationOrToastMock: vi.fn(),
  addRecentPathStateMock: vi.fn(() => Promise.resolve()),
  addToastMock: vi.fn(() => 'toast-id'),
  getEffectiveShortcutsMock: vi.fn(() => ['⌘[']),
  navigateToDirMock: vi.fn(() => Promise.resolve()),
  navigateToFileMock: vi.fn(() => Promise.resolve()),
  getFocusedPaneMock: vi.fn(() => 'left'),
  getFocusedPanePathMock: vi.fn(() => '/home/me'),
}))

/** A resolved `Location` echoing the dir onto the `root` volume (the shared resolve-or-toast). */
function okLocation(path: string) {
  return { volumeId: 'root', path }
}

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    resolveGoToPath: resolveGoToPathMock,
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

vi.mock('$lib/shortcuts', async () => ({
  getEffectiveShortcuts: getEffectiveShortcutsMock,
  toDisplayShortcut: (await import('$lib/shortcuts/key-capture')).toDisplayShortcut,
}))

vi.mock('$lib/file-explorer/navigation/navigate-and-select', () => ({
  navigateToDirInPane: navigateToDirMock,
  navigateToFileInPane: navigateToFileMock,
  resolveLocationOrToast: resolveLocationOrToastMock,
}))

vi.mock('./recent-paths-state.svelte', () => ({
  addRecentPath: addRecentPathStateMock,
}))

// The scheme intercept is its own module with its own suite; here we only pin
// that the jump asks it FIRST and acts on what it says.
vi.mock('./scheme-intercept', () => ({
  readSchemeInput: readSchemeInputMock,
  actOnSchemeInput: actOnSchemeInputMock,
}))

// `goToPath` reads the focused pane's path from the explorer store, not the
// `explorerRef` getter, so the base dir comes through this module.
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPanePath: getFocusedPanePathMock,
}))

// The toast component import returns an opaque module ref; we only assert the
// same reference reaches `addToast`.
vi.mock('./GoToPathAncestorToastContent.svelte', () => ({
  default: { __toastContent: 'GoToPathAncestorToastContent' },
}))

import { goToPath, digitToRecentIndex, shouldPrefillClipboard } from './go-to-path'
import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'
import type { GoToPathResolution } from '$lib/ipc/bindings'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

function makeExplorerStub(): ExplorerAPI {
  return {
    getFocusedPane: getFocusedPaneMock,
  } as unknown as ExplorerAPI
}

function okResolve(data: GoToPathResolution) {
  resolveGoToPathMock.mockResolvedValue({ status: 'ok', data })
}

describe('goToPath handler', () => {
  beforeEach(() => {
    resolveGoToPathMock.mockReset()
    // By default every dir resolves to a `Location` on the `root` volume.
    resolveLocationOrToastMock.mockReset().mockImplementation((path: string) => Promise.resolve(okLocation(path)))
    addRecentPathStateMock.mockReset().mockResolvedValue(undefined)
    addToastMock.mockReset().mockReturnValue('toast-id')
    getEffectiveShortcutsMock.mockReset().mockReturnValue(['⌘['])
    navigateToDirMock.mockReset().mockResolvedValue(undefined)
    navigateToFileMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
    getFocusedPanePathMock.mockReset().mockReturnValue('/home/me')
    readSchemeInputMock.mockReset().mockResolvedValue(null)
    actOnSchemeInputMock.mockReset()
  })

  describe('a pasted server address', () => {
    it('navigates to a saved place, and ❌ never asks the local resolver', async () => {
      const path = 'sftp://ada@nas.local:22/srv/data'
      readSchemeInputMock.mockResolvedValue({ kind: 'place', path, label: 'Naspolya' })
      actOnSchemeInputMock.mockResolvedValue({ kind: 'directory', path })

      const outcome = await goToPath(makeExplorerStub(), path)

      expect(outcome).toEqual({ kind: 'directory', path })
      expect(navigateToDirMock).toHaveBeenCalledWith(expect.anything(), 'left', { volumeId: 'root', path })
      expect(addRecentPathStateMock).toHaveBeenCalled()
      // ❗ `resolve_go_to_path` walks `std::fs::metadata`: given this it would
      // answer `invalid`, and the user would be told their NAS doesn't exist.
      expect(resolveGoToPathMock).not.toHaveBeenCalled()
    })

    it('hands an unsaved address to the sheet, and navigates nowhere', async () => {
      readSchemeInputMock.mockResolvedValue({ kind: 'add', address: 'smb://naspolya' })
      actOnSchemeInputMock.mockResolvedValue({ kind: 'handed_off' })

      const outcome = await goToPath(makeExplorerStub(), 'smb://naspolya')

      expect(outcome).toEqual({ kind: 'handed_off' })
      expect(navigateToDirMock).not.toHaveBeenCalled()
      expect(resolveGoToPathMock).not.toHaveBeenCalled()
    })
  })

  it('resolves against the focused pane path', async () => {
    okResolve({ kind: 'directory', path: '/tmp' })
    await goToPath(makeExplorerStub(), '/tmp')
    expect(resolveGoToPathMock).toHaveBeenCalledWith('/tmp', '/home/me')
  })

  it('directory → resolves the dir volume + navigateToDirInPane + records the dir', async () => {
    okResolve({ kind: 'directory', path: '/tmp/here' })
    await goToPath(makeExplorerStub(), '/tmp/here')
    expect(resolveLocationOrToastMock).toHaveBeenCalledWith('/tmp/here')
    expect(navigateToDirMock).toHaveBeenCalledWith(expect.anything(), 'left', { volumeId: 'root', path: '/tmp/here' })
    expect(navigateToFileMock).not.toHaveBeenCalled()
    expect(addRecentPathStateMock).toHaveBeenCalledWith(expect.objectContaining({ path: '/tmp/here' }))
  })

  it('file → resolves the parent volume + navigateToFileInPane + records the backend-authoritative path', async () => {
    okResolve({ kind: 'file', path: '/tmp/a.txt', parentDir: '/tmp', fileName: 'a.txt' })
    await goToPath(makeExplorerStub(), '/tmp/a.txt')
    expect(resolveLocationOrToastMock).toHaveBeenCalledWith('/tmp')
    expect(navigateToFileMock).toHaveBeenCalledWith(
      expect.anything(),
      'left',
      { volumeId: 'root', path: '/tmp' },
      'a.txt',
    )
    expect(navigateToDirMock).not.toHaveBeenCalled()
    expect(addRecentPathStateMock).toHaveBeenCalledWith(expect.objectContaining({ path: '/tmp/a.txt' }))
  })

  it('nearestAncestor → navigates to the ancestor + records the ancestor', async () => {
    okResolve({ kind: 'nearestAncestor', requested: '/tmp/nope/a.txt', ancestorDir: '/tmp' })
    await goToPath(makeExplorerStub(), '/tmp/nope/a.txt')
    expect(navigateToDirMock).toHaveBeenCalledWith(expect.anything(), 'left', { volumeId: 'root', path: '/tmp' })
    expect(addRecentPathStateMock).toHaveBeenCalledWith(expect.objectContaining({ path: '/tmp' }))
  })

  it('unresolvable volume → no navigation, no recents (the shared helper owns the toast)', async () => {
    okResolve({ kind: 'directory', path: '/Volumes/Gone/dir' })
    resolveLocationOrToastMock.mockResolvedValue(null)
    await goToPath(makeExplorerStub(), '/Volumes/Gone/dir')
    expect(navigateToDirMock).not.toHaveBeenCalled()
    expect(navigateToFileMock).not.toHaveBeenCalled()
    expect(addRecentPathStateMock).not.toHaveBeenCalled()
  })

  it('nearestAncestor → builds the toast with the SNAPSHOTTED nav.back shortcut', async () => {
    // A non-default binding proves the toast reads the live effective shortcut
    // rather than hardcoding `⌘[`.
    getEffectiveShortcutsMock.mockReturnValue(['⌘B'])
    okResolve({ kind: 'nearestAncestor', requested: '/x/y', ancestorDir: '/' })
    await goToPath(makeExplorerStub(), '/x/y')

    expect(getEffectiveShortcutsMock).toHaveBeenCalledWith('nav.back')
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock).toHaveBeenCalledWith(
      GoToPathAncestorToastContent,
      expect.objectContaining({
        level: 'info',
        props: { requested: '/x/y', landed: '/', backShortcut: '⌘B' },
      }),
    )
  })

  it('invalid → no navigation, no recents, no toast', async () => {
    okResolve({ kind: 'invalid', reason: 'empty' })
    const resolution = await goToPath(makeExplorerStub(), '')
    expect(navigateToDirMock).not.toHaveBeenCalled()
    expect(navigateToFileMock).not.toHaveBeenCalled()
    expect(addRecentPathStateMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
    expect(resolution?.kind).toBe('invalid')
  })

  it('no explorer → no-op, returns undefined', async () => {
    const resolution = await goToPath(undefined, '/tmp')
    expect(resolution).toBeUndefined()
    expect(resolveGoToPathMock).not.toHaveBeenCalled()
  })

  it('resolve error → no navigation, returns undefined', async () => {
    resolveGoToPathMock.mockResolvedValue({ status: 'error', error: { message: 'boom' } })
    const resolution = await goToPath(makeExplorerStub(), '/tmp')
    expect(resolution).toBeUndefined()
    expect(navigateToDirMock).not.toHaveBeenCalled()
  })
})

describe('digitToRecentIndex', () => {
  it("maps '1'..'9' to 0..8 and '0' to 9 when the box is empty", () => {
    expect(digitToRecentIndex('', '1', 10)).toBe(0)
    expect(digitToRecentIndex('', '9', 10)).toBe(8)
    expect(digitToRecentIndex('', '0', 10)).toBe(9)
    expect(digitToRecentIndex('', '5', 10)).toBe(4)
  })

  it('returns null when the index is out of range', () => {
    expect(digitToRecentIndex('', '3', 2)).toBeNull()
    expect(digitToRecentIndex('', '0', 9)).toBeNull()
    expect(digitToRecentIndex('', '1', 0)).toBeNull()
  })

  it('returns null when the box is non-empty (digits are ordinary input)', () => {
    expect(digitToRecentIndex('/tmp', '1', 10)).toBeNull()
    expect(digitToRecentIndex('a', '0', 10)).toBeNull()
  })

  it('returns null for non-digit keys', () => {
    expect(digitToRecentIndex('', 'a', 10)).toBeNull()
    expect(digitToRecentIndex('', 'Enter', 10)).toBeNull()
    expect(digitToRecentIndex('', '/', 10)).toBeNull()
  })

  it('returns null when a modifier is held', () => {
    expect(digitToRecentIndex('', '1', 10, true)).toBeNull()
  })
})

describe('shouldPrefillClipboard', () => {
  it('is true for directory and file resolutions', () => {
    expect(shouldPrefillClipboard({ kind: 'directory', path: '/x' })).toBe(true)
    expect(shouldPrefillClipboard({ kind: 'file', path: '/x/a', parentDir: '/x', fileName: 'a' })).toBe(true)
  })

  it('is false for nearestAncestor and invalid resolutions', () => {
    expect(shouldPrefillClipboard({ kind: 'nearestAncestor', requested: '/x/y', ancestorDir: '/x' })).toBe(false)
    expect(shouldPrefillClipboard({ kind: 'invalid', reason: 'empty' })).toBe(false)
  })
})
