import { describe, it, expect } from 'vitest'
import { canOpenTerminalIn, resolveTerminalFolder, type TerminalTargetPane } from './terminal-target'
import type { VolumeKind } from '#lib/file-explorer/pane/volume-capabilities.js'

function pane(overrides: Partial<TerminalTargetPane> = {}): TerminalTargetPane {
  return {
    panePath: '/Users/test_user_10/code',
    volumeKind: 'local',
    cursorEntry: null,
    virtualGitPortalOn: true,
    ...overrides,
  }
}

describe('canOpenTerminalIn', () => {
  it('says yes for a local volume and for a share, whose paths a shell can reach', () => {
    // SMB counts because both the OS-mounted and the direct-smb2 case keep an
    // ordinary `/Volumes/…` mount alive. Rust reads it the same way.
    expect(canOpenTerminalIn('local')).toBe(true)
    expect(canOpenTerminalIn('smb')).toBe(true)
  })

  it('says no for the device kinds, the two server kinds, and the two virtual kinds', () => {
    // SFTP and WebDAV keep no OS mount, so `cd` has nowhere to go; without their
    // own kind they would classify as `smb` and fire with an `sftp://` path.
    const refused: VolumeKind[] = ['mtp', 'adb', 'sftp', 'webdav', 'network', 'search-results']
    for (const kind of refused) {
      expect(canOpenTerminalIn(kind), kind).toBe(false)
    }
  })
})

describe('resolveTerminalFolder', () => {
  it('opens the pane folder when the cursor is on a file', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'notes.md', path: '/Users/test_user_10/code/notes.md', isDirectory: false } }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('opens the folder under the cursor when the cursor is on one', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'src', path: '/Users/test_user_10/code/src', isDirectory: true } }),
    )
    expect(folder).toBe('/Users/test_user_10/code/src')
  })

  it('opens the pane folder on the `..` row, not the parent', () => {
    // Standing on `..` means "I'm looking at this folder", the same reading
    // `getPathToCopyUnderCursor` takes.
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: '..', path: '/Users/test_user_10', isDirectory: true } }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('opens the pane folder when the pane is empty and has no cursor row', () => {
    expect(resolveTerminalFolder(pane({ cursorEntry: null }))).toBe('/Users/test_user_10/code')
  })

  it('opens the folder holding the archive when the pane is inside one', () => {
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/test_user_10/code/photos.zip/2026/june',
        cursorEntry: { name: 'a.jpg', path: '/Users/test_user_10/code/photos.zip/2026/june/a.jpg', isDirectory: false },
      }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('opens the folder holding the archive even with a folder under the cursor inside it', () => {
    // Nothing inside an archive is on disk, so the inner folder is not a place a
    // shell can stand.
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/test_user_10/code/photos.zip',
        cursorEntry: { name: '2026', path: '/Users/test_user_10/code/photos.zip/2026', isDirectory: true },
      }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('opens the pane folder when the cursor sits on an archive file', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'photos.zip', path: '/Users/test_user_10/code/photos.zip', isDirectory: false } }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('resolves an archive at the filesystem root to `/`', () => {
    const folder = resolveTerminalFolder(pane({ panePath: '/big.zip/inner' }))
    expect(folder).toBe('/')
  })

  it("opens the repo folder when the pane is inside a restic repo's snapshot view", () => {
    // Nothing under `Browse snapshots` is on disk; the repo folder is the nearest place.
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Volumes/testshare/backups/m3/Browse snapshots/latest/Users',
        cursorEntry: {
          name: 'me',
          path: '/Volumes/testshare/backups/m3/Browse snapshots/latest/Users/me',
          isDirectory: true,
        },
      }),
    )
    expect(folder).toBe('/Volumes/testshare/backups/m3')
  })

  it('opens the repo folder itself when the cursor sits on its `Browse snapshots` row', () => {
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/test_user_10/backup',
        cursorEntry: {
          name: 'Browse snapshots',
          path: '/Users/test_user_10/backup/Browse snapshots',
          isDirectory: true,
        },
      }),
    )
    expect(folder).toBe('/Users/test_user_10/backup')
  })

  it('takes the outermost container when an archive and a snapshot view nest', () => {
    expect(resolveTerminalFolder(pane({ panePath: '/r/Browse snapshots/latest/a.zip/x' }))).toBe('/r')
    expect(resolveTerminalFolder(pane({ panePath: '/d/b.zip/r/Browse snapshots/latest' }))).toBe('/d')
  })

  it('opens the working tree when the pane is inside the virtual `.git` portal', () => {
    // `.git/branches/main/src` is a snapshot of history, not a folder on disk; a shell
    // handed it lands nowhere. The repo's working tree is where git commands run.
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/test_user_10/code/.git/branches/main/src',
        cursorEntry: { name: 'lib', path: '/Users/test_user_10/code/.git/branches/main/src/lib', isDirectory: true },
      }),
    )
    expect(folder).toBe('/Users/test_user_10/code')
  })

  it('opens the `.git` folder itself, not a portal category under the cursor', () => {
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/test_user_10/code/.git',
        cursorEntry: { name: 'branches', path: '/Users/test_user_10/code/.git/branches', isDirectory: true },
      }),
    )
    expect(folder).toBe('/Users/test_user_10/code/.git')
  })

  it('treats `.git/branches` as a real folder while the portal is switched off', () => {
    const folder = resolveTerminalFolder(
      pane({ panePath: '/Users/test_user_10/code/.git/branches/old', virtualGitPortalOn: false }),
    )
    expect(folder).toBe('/Users/test_user_10/code/.git/branches/old')
  })

  it('refuses a pane whose volume hands out no OS-visible paths', () => {
    expect(resolveTerminalFolder(pane({ volumeKind: 'mtp', panePath: 'mtp://phone/DCIM' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'adb', panePath: 'adb://phone/sdcard' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'search-results' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'network', panePath: 'smb://nas' }))).toBeNull()
  })

  it('opens a folder on a mounted share', () => {
    const folder = resolveTerminalFolder(pane({ volumeKind: 'smb', panePath: '/Volumes/testshare/documents' }))
    expect(folder).toBe('/Volumes/testshare/documents')
  })
})
