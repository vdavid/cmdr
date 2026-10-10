/**
 * Tests for the pure transfer-dialog derivation helpers (path validation and
 * free-space formatting). No reactivity, no IPC — these are the testable
 * branches lifted out of `TransferDialog.svelte`.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { getPathValidationError, formatSpaceInfo } from './transfer-dialog-logic'
import type { SpaceInfo } from '$lib/ipc/bindings'

// Pin the base locale so the catalog-resolved validation/space copy is the
// deterministic en parity net.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** Both sides on the boot volume, where a path box spelling is the absolute path. */
const BOOT = { sourceVolumePath: '/', destinationVolumePath: '/' }
/** Both sides on a drive mounted under `/Volumes`: the panes hold absolute paths, the path box a volume-relative one. */
const STICK = { sourceVolumePath: '/Volumes/Stick', destinationVolumePath: '/Volumes/Stick' }

describe('getPathValidationError', () => {
  it('returns null when the destination is unrelated to the sources', () => {
    expect(getPathValidationError(['/a/photos'], '/b/dest', 'copy', BOOT)).toBeNull()
  })

  it('rejects copying a folder into itself', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'copy', BOOT)).toBe(
      `Can’t copy “photos” into its own subfolder`,
    )
  })

  it('rejects copying a folder into its own subfolder', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos/sub', 'copy', BOOT)).toBe(
      `Can’t copy “photos” into its own subfolder`,
    )
  })

  it('uses the move verb for a move operation', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'move', BOOT)).toBe(
      `Can’t move “photos” into its own subfolder`,
    )
  })

  it('accepts copying into the source own parent, which duplicates it', () => {
    expect(getPathValidationError(['/a/photos'], '/a', 'copy', BOOT)).toBeNull()
  })

  it('still rejects moving into the source own parent, which would do nothing', () => {
    expect(getPathValidationError(['/a/photos'], '/a', 'move', BOOT)).toBe(`“photos” is already in this location`)
  })

  it('normalizes trailing slashes on both sides before comparing', () => {
    expect(getPathValidationError(['/a/photos/'], '/a/photos', 'copy', BOOT)).toBe(
      `Can’t copy “photos” into its own subfolder`,
    )
    expect(getPathValidationError(['/a/photos'], '/a/', 'move', BOOT)).toBe(`“photos” is already in this location`)
  })

  it('flags any matching source when several are given', () => {
    expect(getPathValidationError(['/a/notes.txt', '/a/photos'], '/a/photos/sub', 'copy', BOOT)).toBe(
      `Can’t copy “photos” into its own subfolder`,
    )
  })

  it('does not falsely match a sibling with a shared name prefix', () => {
    // "/a/photos2" must not count as inside "/a/photos".
    expect(getPathValidationError(['/a/photos'], '/a/photos2', 'copy', BOOT)).toBeNull()
  })

  it('prioritizes the subfolder error over the already-in-location error', () => {
    // The destination equals the source AND (vacuously) is its own parent path
    // only in the subfolder branch; assert the subfolder branch wins for an
    // exact match.
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'move', BOOT)).toContain('into its own subfolder')
  })

  it('keeps the subfolder rejection for a copy, which is the real hazard', () => {
    // Copying a folder into its own subtree recurses until the disk fills, so
    // that guard stays even though the same-folder one is gone.
    expect(getPathValidationError(['/a/photos'], '/a/photos/sub', 'copy', BOOT)).toContain('into its own subfolder')
  })

  describe('on a drive mounted under /Volumes', () => {
    it('rejects copying a folder into its own subfolder, typed volume-relative', () => {
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/photos/sub', 'copy', STICK)).toBe(
        `Can’t copy “photos” into its own subfolder`,
      )
    })

    it('rejects copying a folder into itself, with or without trailing slashes', () => {
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/photos', 'copy', STICK)).toContain(
        'into its own subfolder',
      )
      expect(getPathValidationError(['/Volumes/Stick/photos/'], '/photos/', 'move', STICK)).toContain(
        'into its own subfolder',
      )
    })

    it('rejects the destination spelled absolute too', () => {
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/Volumes/Stick/photos/sub', 'copy', STICK)).toContain(
        'into its own subfolder',
      )
    })

    it('rejects a move into the folder the source already sits in', () => {
      expect(getPathValidationError(['/Volumes/Stick/a/photos'], '/a', 'move', STICK)).toBe(
        `“photos” is already in this location`,
      )
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/', 'move', STICK)).toBe(
        `“photos” is already in this location`,
      )
    })

    it('does not match a sibling volume sharing the name prefix', () => {
      const roots = { sourceVolumePath: '/Volumes/Stick', destinationVolumePath: '/Volumes/Stick 2' }
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/photos/sub', 'copy', roots)).toBeNull()
    })
  })

  describe('across volumes', () => {
    it('does not refuse a boot-volume folder whose path the other drive happens to repeat', () => {
      const roots = { sourceVolumePath: '/', destinationVolumePath: '/Volumes/Stick' }
      expect(getPathValidationError(['/photos'], '/photos/sub', 'copy', roots)).toBeNull()
      expect(getPathValidationError(['/a/photos'], '/a', 'move', roots)).toBeNull()
    })

    it('still refuses a boot-volume source when the destination box names a path inside it', () => {
      // Picking the boot volume and typing a path inside the source is the same folder.
      const roots = { sourceVolumePath: '/Volumes/Stick', destinationVolumePath: '/' }
      expect(getPathValidationError(['/Volumes/Stick/photos'], '/Volumes/Stick/photos/sub', 'copy', roots)).toContain(
        'into its own subfolder',
      )
    })
  })

  describe('compress mode', () => {
    it('accepts a target path ending in .zip', () => {
      expect(getPathValidationError(['/a/photos'], '/b/photos.zip', 'compress', BOOT)).toBeNull()
    })

    it('accepts .zip regardless of case', () => {
      expect(getPathValidationError(['/a/photos'], '/b/photos.ZIP', 'compress', BOOT)).toBeNull()
    })

    it('rejects a target that does not end in .zip', () => {
      expect(getPathValidationError(['/a/photos'], '/b/photos.tar', 'compress', BOOT)).toBe(
        'The archive name must end in “.zip”.',
      )
    })

    it('rejects a bare ".zip" with no archive name', () => {
      expect(getPathValidationError(['/a/photos'], '/b/.zip', 'compress', BOOT)).toBe(
        'The archive name must end in “.zip”.',
      )
    })

    it('does NOT apply the copy/move subfolder rule (compress makes one new file)', () => {
      // The target sits inside a source folder — forbidden for copy/move, fine for
      // compress (it's a distinct new archive, not the folder moving into itself).
      expect(getPathValidationError(['/a/photos'], '/a/photos/backup.zip', 'compress', BOOT)).toBeNull()
    })
  })
})

describe('formatSpaceInfo', () => {
  const fmt = (n: number): string => `${String(n)} B`

  it('returns an empty string when no space info is available', () => {
    expect(formatSpaceInfo(null, fmt)).toBe('')
  })

  it('formats free-of-total using the injected formatter', () => {
    const space: SpaceInfo = { kind: 'bounded', availableBytes: 500, totalBytes: 1000, usedBytes: 500 }
    expect(formatSpaceInfo(space, fmt)).toBe('500 B free of 1000 B')
  })

  it('states what is stored, and that nothing caps it, where there is no ceiling', () => {
    // ❗ The dialog has to say the copy will FIT. A blank line here, or a
    // "0 B free" one, would read as a destination with no room.
    const space: SpaceInfo = { kind: 'unbounded', usedBytes: 64_000_000 }
    expect(formatSpaceInfo(space, fmt)).toBe('64000000 B used, no size limit')
  })
})

describe('complete destination validation', () => {
  it.each(['copy', 'move'] as const)('rejects the identical %s target', (operationType) => {
    expect(getPathValidationError(['/a/notes.txt'], '/a/notes.txt', operationType, BOOT, true)).toBe(
      '“notes.txt” is already in this location',
    )
  })
  it.each(['copy', 'move'] as const)('allows %s with a different name in the source folder', (operationType) => {
    expect(getPathValidationError(['/a/notes.txt'], '/a/backup.txt', operationType, BOOT, true)).toBeNull()
  })
  it.each(['copy', 'move'] as const)('keeps the %s subtree guard for a complete target', (operationType) => {
    expect(getPathValidationError(['/a/folder'], '/a/folder/new-folder', operationType, BOOT, true)).not.toBeNull()
  })
})
