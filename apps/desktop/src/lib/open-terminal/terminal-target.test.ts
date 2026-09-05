import { describe, it, expect } from 'vitest'
import { canOpenTerminalIn, resolveTerminalFolder, type TerminalTargetPane } from './terminal-target'
import type { VolumeKind } from '$lib/file-explorer/pane/volume-capabilities'

function pane(overrides: Partial<TerminalTargetPane> = {}): TerminalTargetPane {
  return {
    panePath: '/Users/dave/code',
    volumeKind: 'local',
    cursorEntry: null,
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

  it('says no for the device kinds and the two virtual kinds', () => {
    const refused: VolumeKind[] = ['mtp', 'adb', 'network', 'search-results']
    for (const kind of refused) {
      expect(canOpenTerminalIn(kind), kind).toBe(false)
    }
  })
})

describe('resolveTerminalFolder', () => {
  it('opens the pane folder when the cursor is on a file', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'notes.md', path: '/Users/dave/code/notes.md', isDirectory: false } }),
    )
    expect(folder).toBe('/Users/dave/code')
  })

  it('opens the folder under the cursor when the cursor is on one', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'src', path: '/Users/dave/code/src', isDirectory: true } }),
    )
    expect(folder).toBe('/Users/dave/code/src')
  })

  it('opens the pane folder on the `..` row, not the parent', () => {
    // Standing on `..` means "I'm looking at this folder", the same reading
    // `getPathToCopyUnderCursor` takes.
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: '..', path: '/Users/dave', isDirectory: true } }),
    )
    expect(folder).toBe('/Users/dave/code')
  })

  it('opens the pane folder when the pane is empty and has no cursor row', () => {
    expect(resolveTerminalFolder(pane({ cursorEntry: null }))).toBe('/Users/dave/code')
  })

  it('opens the folder holding the archive when the pane is inside one', () => {
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/dave/code/photos.zip/2026/june',
        cursorEntry: { name: 'a.jpg', path: '/Users/dave/code/photos.zip/2026/june/a.jpg', isDirectory: false },
      }),
    )
    expect(folder).toBe('/Users/dave/code')
  })

  it('opens the folder holding the archive even with a folder under the cursor inside it', () => {
    // Nothing inside an archive is on disk, so the inner folder is not a place a
    // shell can stand.
    const folder = resolveTerminalFolder(
      pane({
        panePath: '/Users/dave/code/photos.zip',
        cursorEntry: { name: '2026', path: '/Users/dave/code/photos.zip/2026', isDirectory: true },
      }),
    )
    expect(folder).toBe('/Users/dave/code')
  })

  it('opens the pane folder when the cursor sits on an archive file', () => {
    const folder = resolveTerminalFolder(
      pane({ cursorEntry: { name: 'photos.zip', path: '/Users/dave/code/photos.zip', isDirectory: false } }),
    )
    expect(folder).toBe('/Users/dave/code')
  })

  it('resolves an archive at the filesystem root to `/`', () => {
    const folder = resolveTerminalFolder(pane({ panePath: '/big.zip/inner' }))
    expect(folder).toBe('/')
  })

  it('refuses a pane whose volume hands out no OS-visible paths', () => {
    expect(resolveTerminalFolder(pane({ volumeKind: 'mtp', panePath: 'mtp://phone/DCIM' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'adb', panePath: 'adb://phone/sdcard' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'search-results' }))).toBeNull()
    expect(resolveTerminalFolder(pane({ volumeKind: 'network', panePath: 'smb://nas' }))).toBeNull()
  })

  it('opens a folder on a mounted share', () => {
    const folder = resolveTerminalFolder(pane({ volumeKind: 'smb', panePath: '/Volumes/naspi/papers' }))
    expect(folder).toBe('/Volumes/naspi/papers')
  })
})
