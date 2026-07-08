/**
 * Tests for the per-kind volume-capability table + the unified classifier.
 *
 * Three concerns:
 *  1. The frozen per-kind table: each kind maps to its exact row + frozenness + purity.
 *  2. `volumeKindOf`: every real/virtual input classifies correctly, including
 *     the favorite edge and the real-but-unclassified default (totality).
 *  3. `capabilitiesFor`: the store-reading convenience, including the
 *     store-lookup-miss path (virtual ids + stale ids).
 *
 * The classifier-unify byte-stability (`volumeKindFor` / tint still returns
 * `'other'` for the two virtual kinds) is pinned in `volume-tint.test.ts` and
 * the tint-render suites; those must stay green alongside this file.
 */

import { describe, it, expect, vi } from 'vitest'
import type { VolumeInfo } from '$lib/file-explorer/types'

// `capabilitiesFor` reads the volume store to resolve fsType/category from a
// bare volumeId. Mock it so the test controls the volume list.
const volumes = vi.hoisted(() => ({ list: [] as VolumeInfo[] }))
vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => volumes.list,
}))

import {
  type VolumeKind,
  type VolumeCapabilities,
  volumeKindOf,
  capabilitiesForKind,
  capabilitiesFor,
  capabilitiesForPane,
  pathInsideArchive,
  archiveNameFromPath,
} from './volume-capabilities'

function vol(partial: Partial<VolumeInfo> & { id: string }): VolumeInfo {
  return {
    name: partial.id,
    path: '/',
    category: 'main_volume',
    isEjectable: false,
    ...partial,
  }
}

describe('capabilitiesForKind — the frozen per-kind table', () => {
  const expected: Record<VolumeKind, VolumeCapabilities> = {
    local: {
      kind: 'local',
      hasBackendListing: true,
      canPasteInto: true,
      canCreateChild: true,
      canRenameInPlace: true,
      canBeSource: true,
      supportsSystemClipboard: true,
      hasParentRow: true,
      syncsToMcp: true,
      pathScheme: 'filesystem',
    },
    smb: {
      kind: 'smb',
      hasBackendListing: true,
      canPasteInto: true,
      canCreateChild: true,
      canRenameInPlace: true,
      canBeSource: true,
      supportsSystemClipboard: true,
      hasParentRow: true,
      syncsToMcp: true,
      pathScheme: 'smb',
    },
    mtp: {
      kind: 'mtp',
      hasBackendListing: true,
      canPasteInto: true,
      canCreateChild: true,
      canRenameInPlace: true,
      canBeSource: true,
      supportsSystemClipboard: false,
      hasParentRow: true,
      syncsToMcp: true,
      pathScheme: 'mtp',
    },
    network: {
      kind: 'network',
      hasBackendListing: false,
      canPasteInto: false,
      canCreateChild: false,
      canRenameInPlace: false,
      canBeSource: false,
      supportsSystemClipboard: false,
      hasParentRow: false,
      syncsToMcp: false,
      pathScheme: 'smb',
    },
    'search-results': {
      kind: 'search-results',
      hasBackendListing: false,
      canPasteInto: false,
      canCreateChild: false,
      canRenameInPlace: false,
      canBeSource: true,
      supportsSystemClipboard: false,
      hasParentRow: false,
      syncsToMcp: false,
      pathScheme: 'search-results',
    },
    archive: {
      kind: 'archive',
      hasBackendListing: true,
      // Zip is writable: the three write flags are true (managed archive-edit flow).
      canPasteInto: true,
      canCreateChild: true,
      canRenameInPlace: true,
      canBeSource: true,
      supportsSystemClipboard: false,
      hasParentRow: true,
      syncsToMcp: true,
      pathScheme: 'filesystem',
    },
  }

  for (const kind of Object.keys(expected) as VolumeKind[]) {
    it(`returns the exact row for "${kind}"`, () => {
      expect(capabilitiesForKind(kind)).toEqual(expected[kind])
    })
  }

  it('the search-results row generalizes the original search-results capability seed', () => {
    // seed: { canPasteInto: false, canMkdir: false, canMkfile: false, canRename: false, isSourceOK: true }
    const caps = capabilitiesForKind('search-results')
    expect(caps.canPasteInto).toBe(false) // = seed canPasteInto
    expect(caps.canCreateChild).toBe(false) // folds seed canMkdir + canMkfile
    expect(caps.canRenameInPlace).toBe(false) // = seed canRename
    expect(caps.canBeSource).toBe(true) // = seed isSourceOK
  })

  it('returns a FROZEN reference (no allocation, no mutation)', () => {
    const caps = capabilitiesForKind('local')
    expect(Object.isFrozen(caps)).toBe(true)
    // Same reference on repeated calls (by-reference, no allocation).
    expect(capabilitiesForKind('local')).toBe(caps)
    expect(() => {
      // Mutating a frozen capability throws in strict mode (vitest runs ESM strict).
      ;(caps as { canPasteInto: boolean }).canPasteInto = false
    }).toThrow()
  })

  it('is pure: repeated calls return equal values for every kind', () => {
    for (const kind of Object.keys(expected) as VolumeKind[]) {
      expect(capabilitiesForKind(kind)).toEqual(capabilitiesForKind(kind))
    }
  })
})

describe('volumeKindOf — the unified superset classifier', () => {
  it('classifies the two virtual ids first', () => {
    expect(volumeKindOf('network', undefined, 'network')).toBe('network')
    expect(volumeKindOf('search-results', undefined, undefined)).toBe('search-results')
  })

  it('classifies the real kinds the same as the tint classifier', () => {
    expect(volumeKindOf('root', 'apfs', 'main_volume')).toBe('local')
    expect(volumeKindOf('attached-1', 'exfat', 'attached_volume')).toBe('local')
    expect(volumeKindOf('icloud', 'apfs', 'cloud_drive')).toBe('local')
    expect(volumeKindOf('volumesnaspi', 'smbfs', 'network')).toBe('smb')
    expect(volumeKindOf('some-id', 'smbfs', undefined)).toBe('smb')
    expect(volumeKindOf('mtp-336592896:65537', undefined, 'mobile_device')).toBe('mtp')
    expect(volumeKindOf('0-5:65537', undefined, undefined)).toBe('mtp')
  })

  it('the favorite edge resolves to its containing real volume kind (local)', () => {
    // A favorite is a virtual id pointing at a real path; the tint classifier
    // returns `'other'` (untinted) for it, but a capability lookup must yield a
    // sane real-kind row. Default → `local`.
    expect(volumeKindOf('fav-downloads', undefined, 'favorite')).toBe('local')
  })

  it('is TOTAL: a real-but-unclassified id defaults to local (no `other` escape)', () => {
    // fsType + category both undefined → tint `volumeKindFor` returns `'other'`,
    // which has no capability row. `volumeKindOf` must NOT echo `'other'`.
    const kind = volumeKindOf('mystery', undefined, undefined)
    expect(kind).toBe('local')
    // The lookup must never miss the table.
    expect(capabilitiesForKind(kind)).toBeDefined()
  })

  it('never returns a kind missing from the frozen table, for any input', () => {
    const inputs: Array<[string, string | undefined, VolumeInfo['category'] | undefined]> = [
      ['network', undefined, 'network'],
      ['search-results', undefined, undefined],
      ['root', 'apfs', 'main_volume'],
      ['mtp-1:1', undefined, 'mobile_device'],
      ['x', 'smbfs', undefined],
      ['fav', undefined, 'favorite'],
      ['weird', undefined, undefined],
    ]
    for (const [id, fsType, category] of inputs) {
      expect(capabilitiesForKind(volumeKindOf(id, fsType, category))).toBeDefined()
    }
  })
})

describe('capabilitiesFor — the store-reading convenience', () => {
  it('resolves fsType/category from the volume store for a real id', () => {
    volumes.list = [vol({ id: 'volumesnaspi', fsType: 'smbfs', category: 'network' })]
    expect(capabilitiesFor('volumesnaspi').kind).toBe('smb')
  })

  it('short-circuits the two virtual ids WITHOUT a store entry (lookup miss)', () => {
    volumes.list = [] // neither virtual id is ever in the store
    expect(capabilitiesFor('network').kind).toBe('network')
    expect(capabilitiesFor('search-results').kind).toBe('search-results')
    expect(capabilitiesFor('network').canPasteInto).toBe(false)
  })

  it('falls to the local default for a stale/missing real id (store-lookup miss)', () => {
    volumes.list = [vol({ id: 'other-vol', fsType: 'apfs', category: 'main_volume' })]
    // `stale-id` is not in the store → fsType/category undefined → local default.
    expect(capabilitiesFor('stale-id').kind).toBe('local')
    expect(capabilitiesFor('stale-id')).toBeDefined()
  })

  it('NEVER returns undefined for any input', () => {
    volumes.list = []
    for (const id of ['network', 'search-results', 'root', 'mtp-1:1', 'nope']) {
      expect(capabilitiesFor(id)).toBeDefined()
    }
  })
})

describe('pathInsideArchive — the pure extension-only boundary check', () => {
  it('is true at the archive root and anywhere inside it', () => {
    expect(pathInsideArchive('/a/foo.zip')).toBe(true) // the archive root itself
    expect(pathInsideArchive('/a/foo.zip/inner')).toBe(true)
    expect(pathInsideArchive('/a/foo.zip/inner/deep/file.txt')).toBe(true)
  })

  it('is false for a plain folder that merely CONTAINS an archive', () => {
    // The pane is at `/a`, listing `foo.zip` as a row — not inside it.
    expect(pathInsideArchive('/a')).toBe(false)
    expect(pathInsideArchive('/a/b/c')).toBe(false)
  })

  it('matches the case-insensitive extension, and any component (nested leftmost)', () => {
    expect(pathInsideArchive('/a/foo.ZIP/inner')).toBe(true)
    expect(pathInsideArchive('/a/archive.name.zip')).toBe(true)
    // Leftmost archive component makes the whole path "inside"; the inner b.zip
    // is a plain entry the FE can't distinguish, but the answer (true) is right.
    expect(pathInsideArchive('/a.zip/b.zip/x')).toBe(true)
  })

  it('is NOT decidable-true for a component whose extension is not an archive', () => {
    // `foo.zip.txt`: final extension is txt, so the STRING doesn't cross a boundary.
    expect(pathInsideArchive('/a/foo.zip.txt')).toBe(false)
    // A leading-dot dotfile has no stem, so `.zip` is not an extension.
    expect(pathInsideArchive('/a/.zip')).toBe(false)
    // No dot at all.
    expect(pathInsideArchive('/a/zip/file')).toBe(false)
  })

  it('a real directory literally named foo.zip is NOT decidable here (backend corrects)', () => {
    // Extension-only: the FE reads this as inside-an-archive. The backend
    // stat+magic check corrects a real directory to plain navigation; the FE only
    // uses this for read-only gating, where the false positive is safe.
    expect(pathInsideArchive('/a/foo.zip/anything')).toBe(true)
  })
})

describe('capabilitiesForPane — kind-from-path resolution', () => {
  it('returns the writable archive row when the PATH is inside a zip', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    // The volumeId is the parent drive, but the path crosses a zip — the archive
    // row (writable) gates the pane, not the drive's row.
    const caps = capabilitiesForPane('root', '/Users/me/foo.zip/inner')
    expect(caps.kind).toBe('archive')
    expect(caps.canPasteInto).toBe(true)
    expect(caps.canCreateChild).toBe(true)
    expect(caps.canRenameInPlace).toBe(true)
    expect(caps.canBeSource).toBe(true)
    expect(caps.hasBackendListing).toBe(true)
    // No system clipboard even when writable: archive-inner paths aren't OS URLs.
    expect(caps.supportsSystemClipboard).toBe(false)
  })

  it('defers to the id-based kind when the path is NOT inside an archive', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    expect(capabilitiesForPane('root', '/Users/me/Documents').kind).toBe('local')
  })

  it('defers to the id-based kind when the path is undefined', () => {
    volumes.list = [vol({ id: 'volumesnaspi', fsType: 'smbfs', category: 'network' })]
    expect(capabilitiesForPane('volumesnaspi', undefined).kind).toBe('smb')
  })

  it('returns the READ-ONLY archive row for a tar or 7z path (browse + extract only)', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    for (const path of ['/x/foo.tar/inner', '/x/foo.tar.gz/d/f.txt', '/x/foo.7z/inner']) {
      const caps = capabilitiesForPane('root', path)
      expect(caps.kind, path).toBe('archive')
      // Read-only: the three write flags are off...
      expect(caps.canPasteInto, path).toBe(false)
      expect(caps.canCreateChild, path).toBe(false)
      expect(caps.canRenameInPlace, path).toBe(false)
      // ...but copying files OUT still works, and it lists like a folder.
      expect(caps.canBeSource, path).toBe(true)
      expect(caps.hasBackendListing, path).toBe(true)
    }
  })

  it('a nested zip inside a read-only tar stays read-only (the outer format governs)', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    // Leftmost archive component wins: `foo.tar` is the boundary, `bar.zip` is a
    // plain inner entry — so the pane is read-only, not writable.
    expect(capabilitiesForPane('root', '/x/foo.tar/bar.zip/y').canPasteInto).toBe(false)
  })
})

describe('pathInsideArchive — tar family and 7z', () => {
  it('recognizes the compressed-tar suffixes and 7z', () => {
    for (const name of [
      'a.tar',
      'a.tar.gz',
      'a.tgz',
      'a.tar.xz',
      'a.txz',
      'a.tar.bz2',
      'a.tbz2',
      'a.tar.zst',
      'a.7z',
    ]) {
      expect(pathInsideArchive(`/dir/${name}/inner`), name).toBe(true)
    }
  })

  it('does NOT treat a bare compressed file (not a tar) as an archive', () => {
    expect(pathInsideArchive('/dir/photo.gz')).toBe(false)
    expect(pathInsideArchive('/dir/data.zst/x')).toBe(false)
  })
})

describe('archiveNameFromPath — the archive display name for a prompt', () => {
  it('returns the archive segment for the archive root and any inner path', () => {
    expect(archiveNameFromPath('/a/photos.zip')).toBe('photos.zip')
    expect(archiveNameFromPath('/a/photos.zip/inner/x.jpg')).toBe('photos.zip')
  })

  it('picks the LEFTMOST archive segment (outer archive governs)', () => {
    expect(archiveNameFromPath('/x/foo.tar/bar.zip/y')).toBe('foo.tar')
  })

  it('falls back to the basename when no segment is an archive', () => {
    expect(archiveNameFromPath('/a/b/c.txt')).toBe('c.txt')
  })
})
