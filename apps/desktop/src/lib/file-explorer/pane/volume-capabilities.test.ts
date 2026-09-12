/**
 * Tests for the volume-capability resolution chain.
 *
 * Four concerns:
 *  1. The frozen per-kind defaults: each kind maps to its exact row + frozenness + purity.
 *  2. `volumeKindOf`: every real/virtual input classifies correctly, including
 *     the favorite edge and the real-but-unclassified default (totality).
 *  3. `withBackendCapabilities`: the backend's published answer wins over the
 *     per-kind default, and an absent one leaves the default standing.
 *  4. `capabilitiesFor`: the store-reading convenience, including the
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

// `capabilitiesForPane` reads the live `fileExplorer.git.showVirtualGitPortal`
// switch: with the portal off the backend routes nothing, so a `.git/branches/`
// path is whatever sits on disk and keeps its volume's own row. Default ON,
// matching the setting's registry default.
const gitPortal = vi.hoisted(() => ({ on: true }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getShowVirtualGitPortal: () => gitPortal.on,
}))

import {
  type VolumeKind,
  type VolumeCapabilities,
  volumeKindOf,
  capabilitiesForKind,
  capabilitiesFor,
  capabilitiesForPane,
  withBackendCapabilities,
  paneFolderIsPolledForDeletion,
  paneRowsAreOsVisible,
  rowIsOsVisible,
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

describe('capabilitiesForKind — the frozen per-kind defaults', () => {
  const expected: Record<VolumeKind, VolumeCapabilities> = {
    local: {
      kind: 'local',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: true,
      pollsForDeletedFolder: true,
    },
    smb: {
      kind: 'smb',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: true,
      pollsForDeletedFolder: true,
    },
    sftp: {
      kind: 'sftp',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: false,
      // No OS mount: the Mac can't see the folder, so nothing it watches goes blind.
      pollsForDeletedFolder: false,
    },
    webdav: {
      kind: 'webdav',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: false,
      pollsForDeletedFolder: false,
    },
    mtp: {
      kind: 'mtp',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: true,
      pollsForDeletedFolder: false,
    },
    adb: {
      kind: 'adb',
      hasBackendListing: true,
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      // Indexable like an MTP phone: this default answers for a phone nobody has dialed.
      canBeIndexed: true,
      pollsForDeletedFolder: false,
    },
    network: {
      kind: 'network',
      hasBackendListing: false,
      canWrite: false,
      canBeSource: false,
      hasParentRow: false,
      // No file list at all (NetworkMountView renders instead), so there is
      // nothing for a sort to order.
      syncsToMcp: false,
      canBeIndexed: false,
      pollsForDeletedFolder: false,
    },
    'search-results': {
      kind: 'search-results',
      hasBackendListing: false,
      canWrite: false,
      canBeSource: true,
      hasParentRow: false,
      // The rows arrive in the search engine's ranked order and stay in it:
      // every source-side op resolves a selected index against `snapshot.entries[i]`.
      // Mirrors to MCP off the frontend snapshot: no backend listing needed, and
      // the copy/move/delete gate reads this pane's state.
      syncsToMcp: true,
      canBeIndexed: false,
      pollsForDeletedFolder: false,
    },
    archive: {
      kind: 'archive',
      hasBackendListing: true,
      // Zip is writable through the managed archive-edit flow.
      canWrite: true,
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: false,
      // The archive file itself sits in a folder; the DRIVE it's on still decides.
      pollsForDeletedFolder: true,
    },
    'git-portal': {
      kind: 'git-portal',
      hasBackendListing: true,
      // A snapshot of git history: `GitPortalVolume` keeps the trait's
      // `NotSupported` on every mutation, so nothing here is writable.
      canWrite: false,
      // ...but the rows are real content, so copying OUT works.
      canBeSource: true,
      hasParentRow: true,
      syncsToMcp: true,
      canBeIndexed: false,
      // Snapshot folders never exist on disk, so a poll would evict the user.
      pollsForDeletedFolder: false,
    },
  }

  for (const kind of Object.keys(expected) as VolumeKind[]) {
    it(`returns the exact row for "${kind}"`, () => {
      expect(capabilitiesForKind(kind)).toEqual(expected[kind])
    })
  }

  it('the snapshot pane can be a SOURCE but never a destination', () => {
    // Its rows are real files, so copy/move/delete work off them; there's no
    // folder behind the namespace to write into.
    const caps = capabilitiesForKind('search-results')
    expect(caps.canWrite).toBe(false)
    expect(caps.canBeSource).toBe(true)
  })

  it('returns a FROZEN reference (no allocation, no mutation)', () => {
    const caps = capabilitiesForKind('local')
    expect(Object.isFrozen(caps)).toBe(true)
    // Same reference on repeated calls (by-reference, no allocation).
    expect(capabilitiesForKind('local')).toBe(caps)
    expect(() => {
      // Mutating a frozen capability throws in strict mode (vitest runs ESM strict).
      ;(caps as { canWrite: boolean }).canWrite = false
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
    expect(volumeKindOf('adb-pixel-7-a1b2c3d', 'adb', 'mobile_device')).toBe('adb')
  })

  it('gives SFTP and WebDAV their own kinds, ahead of the network category', () => {
    // A server row is `category: 'network'`, which used to mean SMB and would give
    // an SFTP pane the SMB capability row and an Open-terminal button that fires
    // with an `sftp://` path. `fsType` is what tells the three apart.
    expect(volumeKindOf('sftp-nas-22-ada', 'sftp', 'network')).toBe('sftp')
    expect(volumeKindOf('webdav-cloud-443-ada', 'webdav', 'network')).toBe('webdav')
    expect(volumeKindOf('volumesnaspi', 'smbfs', 'network')).toBe('smb')
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
      ['adb-pixel-7-a1b2c3d', 'adb', 'mobile_device'],
      ['x', 'smbfs', undefined],
      ['sftp-nas-22-ada', 'sftp', 'network'],
      ['webdav-cloud-443-ada', 'webdav', 'network'],
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
    expect(capabilitiesFor('network').canWrite).toBe(false)
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

describe('paneFolderIsPolledForDeletion — which panes cover the FSEvents blind spot', () => {
  // The poll exists because macOS doesn't report a watched folder's own deletion.
  // That blind spot only concerns folders the Mac can see, so every pane on a
  // scheme path answers false, and so does a snapshot that never exists on disk.
  const cases: Array<{ name: string; volume: VolumeInfo; path: string; polled: boolean }> = [
    { name: 'local', volume: vol({ id: 'root', fsType: 'apfs' }), path: '/Users/me/Documents', polled: true },
    {
      name: 'smb',
      volume: vol({ id: 'volumesnaspi', fsType: 'smbfs', category: 'network' }),
      path: '/Volumes/naspi/photos',
      polled: true,
    },
    {
      name: 'sftp',
      volume: vol({ id: 'sftp-nas-22-ada', fsType: 'sftp', category: 'network' }),
      path: 'sftp://nas/home/ada',
      polled: false,
    },
    {
      name: 'webdav',
      volume: vol({ id: 'webdav-cloud-443-ada', fsType: 'webdav', category: 'network' }),
      path: 'webdav://cloud/files',
      polled: false,
    },
    {
      name: 'mtp',
      volume: vol({ id: 'mtp-336592896:65537', category: 'mobile_device' }),
      path: 'mtp://mtp-336592896/65537/DCIM',
      polled: false,
    },
    {
      name: 'adb',
      volume: vol({ id: 'adb-pixel-7-a1b2c3d', fsType: 'adb', category: 'mobile_device' }),
      path: 'adb://R58M1/sdcard/DCIM',
      polled: false,
    },
    { name: 'network', volume: vol({ id: 'network', category: 'network' }), path: 'smb://', polled: false },
    {
      name: 'search-results',
      volume: vol({ id: 'search-results' }),
      path: 'search-results://latest',
      polled: false,
    },
    {
      name: 'a zip on the boot disk',
      volume: vol({ id: 'root', fsType: 'apfs' }),
      path: '/Users/me/photos.zip/2024',
      polled: true,
    },
    {
      name: 'a zip on a phone',
      volume: vol({ id: 'adb-pixel-7-a1b2c3d', fsType: 'adb', category: 'mobile_device' }),
      path: 'adb://R58M1/sdcard/photos.zip/2024',
      polled: false,
    },
    {
      name: 'the virtual `.git` portal',
      volume: vol({ id: 'root', fsType: 'apfs' }),
      path: '/Users/me/repo/.git/branches/main',
      polled: false,
    },
  ]

  for (const { name, volume, path, polled } of cases) {
    it(`${polled ? 'polls' : "doesn't poll"} ${name}`, () => {
      volumes.list = [volume]
      expect(paneFolderIsPolledForDeletion(volume.id, path)).toBe(polled)
    })
  }

  it('polls a real `.git/branches` folder once the portal is switched off', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs' })]
    gitPortal.on = false
    try {
      expect(paneFolderIsPolledForDeletion('root', '/Users/me/repo/.git/branches/main')).toBe(true)
    } finally {
      gitPortal.on = true
    }
  })

  it("doesn't poll a phone or a server whose row has left the volume list", () => {
    // ❗ The case the path half exists for. A pane keeps its `adb://` or `sftp://`
    // path after its row goes (an unplugged phone, a removed server), and a stale
    // server id classifies as `local`, so the kind alone would say "poll" and the
    // boot disk would answer "gone" for a path it can't see.
    volumes.list = []
    expect(paneFolderIsPolledForDeletion('adb-pixel-7-a1b2c3d', 'adb://R58M1/sdcard/DCIM')).toBe(false)
    expect(paneFolderIsPolledForDeletion('sftp-nas-22-ada', 'sftp://nas/home/ada')).toBe(false)
  })
})

describe("withBackendCapabilities — the backend's answer wins over the per-kind default", () => {
  it('leaves the default standing when the backend published nothing', () => {
    const row = capabilitiesForKind('local')
    expect(withBackendCapabilities(row, undefined)).toBe(row)
    expect(withBackendCapabilities(row, null)).toBe(row)
  })

  it('returns the SAME frozen row (no allocation) when the two already agree', () => {
    const row = capabilitiesForKind('local')
    expect(withBackendCapabilities(row, { backendCanWrite: true, canExport: true, canBeIndexed: true })).toBe(row)
  })

  it("takes the backend's answer when it differs, leaving the structural fields alone", () => {
    const row = capabilitiesForKind('local')
    const folded = withBackendCapabilities(row, { backendCanWrite: false, canExport: false, canBeIndexed: true })
    expect(folded.canWrite).toBe(false)
    expect(folded.canBeSource).toBe(false)
    // Kind and the per-namespace UI structure are not the backend's to answer.
    expect(folded.kind).toBe('local')
    expect(folded.hasBackendListing).toBe(true)
    expect(folded.hasParentRow).toBe(true)
    expect(folded.syncsToMcp).toBe(true)
    expect(Object.isFrozen(folded)).toBe(true)
  })

  it("folds the backend's indexability too: a registered volume no index can serve says so", () => {
    const row = capabilitiesForKind('local')
    const folded = withBackendCapabilities(row, { backendCanWrite: true, canExport: true, canBeIndexed: false })
    expect(folded.canBeIndexed).toBe(false)
    expect(folded.canWrite).toBe(true)
    expect(folded.canBeSource).toBe(true)
  })

  it('reaches capabilitiesFor: a backend that declines writes disables them on the pane', () => {
    volumes.list = [
      vol({
        id: 'weird-vol',
        fsType: 'apfs',
        category: 'attached_volume',
        capabilities: { backendCanWrite: false, canExport: true, canBeIndexed: true },
      }),
    ]
    const caps = capabilitiesFor('weird-vol')
    expect(caps.kind).toBe('local')
    expect(caps.canWrite).toBe(false)
    expect(caps.canBeSource).toBe(true)
  })

  it('❌ never lets the backend change the KIND', () => {
    // An OS-mounted SMB share is served by a plain local backend until it's
    // upgraded to smb2. Capability comes from the backend; kind never does.
    volumes.list = [
      vol({
        id: 'volumesnaspi',
        fsType: 'smbfs',
        category: 'network',
        capabilities: { backendCanWrite: true, canExport: true, canBeIndexed: true },
      }),
    ]
    expect(capabilitiesFor('volumesnaspi').kind).toBe('smb')
  })
})

describe('capabilitiesForPane — kind-from-path resolution', () => {
  it('returns the writable archive row when the PATH is inside a zip', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    // The volumeId is the parent drive, but the path crosses a zip — the archive
    // row (writable) gates the pane, not the drive's row.
    const caps = capabilitiesForPane('root', '/Users/me/foo.zip/inner')
    expect(caps.kind).toBe('archive')
    expect(caps.canWrite).toBe(true)
    expect(caps.canBeSource).toBe(true)
    expect(caps.hasBackendListing).toBe(true)
  })

  it("❌ never lets the PARENT drive's published capabilities reach an archive pane", () => {
    // The drive is writable and exports; the pane is inside a tar on it, which is
    // browse + extract only. Folding the drive's answer in here would hand the
    // user an enabled F7 inside a read-only archive.
    volumes.list = [
      vol({
        id: 'root',
        fsType: 'apfs',
        category: 'main_volume',
        capabilities: { backendCanWrite: true, canExport: true, canBeIndexed: true },
      }),
    ]
    expect(capabilitiesForPane('root', '/Users/me/foo.tar/inner').canWrite).toBe(false)
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
      // Read-only: no mutation...
      expect(caps.canWrite, path).toBe(false)
      // ...but copying files OUT still works, and it lists like a folder.
      expect(caps.canBeSource, path).toBe(true)
      expect(caps.hasBackendListing, path).toBe(true)
    }
  })

  it('returns the READ-ONLY archive row inside a DOCUMENT container, on a writable drive', () => {
    // The data-safety property, at the UI layer. A `.docx` is a zip, so nothing
    // about the format stops the mutator — only the refusal does. A user who
    // steps inside a Word file to look around must find every write affordance
    // off, so they can't hand themselves a corrupt document.
    //
    // The drive underneath is deliberately writable and exporting: if this row
    // ever folded in the parent's answer, the pane would go writable and this
    // would catch it. The backend refuses too (`ensure_zip_writable` admits
    // `ArchiveFormat::Zip` alone), so an MCP or IPC caller is stopped as well —
    // this is the visible half of a guarantee, not the whole of it.
    volumes.list = [
      vol({
        id: 'root',
        fsType: 'apfs',
        category: 'main_volume',
        capabilities: { backendCanWrite: true, canExport: true, canBeIndexed: true },
      }),
    ]
    for (const path of [
      '/x/report.docx/word/document.xml',
      '/x/sheet.xlsx/xl',
      '/x/deck.pptx/ppt/slides',
      '/x/lib.jar/META-INF',
      '/x/app.apk/res',
    ]) {
      const caps = capabilitiesForPane('root', path)
      expect(caps.kind, path).toBe('archive')
      expect(caps.canWrite, path).toBe(false)
      // Reading out still works: browse it, copy a part out, preview it.
      expect(caps.canBeSource, path).toBe(true)
      expect(caps.hasBackendListing, path).toBe(true)
    }
  })

  it('keeps a real `.zip` writable, so read-only did not become a blanket refusal', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    expect(capabilitiesForPane('root', '/x/real.zip/inner').canWrite).toBe(true)
  })

  it('returns the read-only git-portal row for a path inside a virtual `.git` category', () => {
    volumes.list = [
      vol({
        id: 'root',
        fsType: 'apfs',
        category: 'main_volume',
        capabilities: { backendCanWrite: true, canExport: true, canBeIndexed: true },
      }),
    ]
    // The volumeId is the writable parent drive; the path crosses `.git/branches/`,
    // so the portal row gates the pane. Without this the UI would offer paste,
    // delete, rename, and new file/folder that the backend refuses.
    const caps = capabilitiesForPane('root', '/Users/me/repo/.git/branches/main/src')
    expect(caps.kind).toBe('git-portal')
    expect(caps.canWrite).toBe(false)
    // Copying out of a snapshot is the headline read feature, and it lists like a folder.
    expect(caps.canBeSource).toBe(true)
    expect(caps.hasBackendListing).toBe(true)
    expect(caps.hasParentRow).toBe(true)
    expect(caps.syncsToMcp).toBe(true)
  })

  it('covers all six virtual categories, and the category directory itself', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    const root = '/Users/me/repo/.git'
    for (const category of ['branches', 'tags', 'commits', 'stash', 'worktrees', 'submodules']) {
      expect(capabilitiesForPane('root', `${root}/${category}`).kind, category).toBe('git-portal')
      expect(capabilitiesForPane('root', `${root}/${category}/thing`).kind, category).toBe('git-portal')
    }
  })

  it("a REAL file under `.git` keeps the parent volume's full row", () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    // `.git/` stays writable: `config`, `HEAD`, and the real `refs/` tree are
    // editable, renamable, and deletable, which the backend defends too. Only
    // the six virtual trees are read-only.
    for (const path of [
      '/Users/me/repo/.git',
      '/Users/me/repo/.git/config',
      '/Users/me/repo/.git/HEAD',
      '/Users/me/repo/.git/refs/heads/main',
      '/Users/me/repo/.git/objects/pack',
    ]) {
      const caps = capabilitiesForPane('root', path)
      expect(caps.kind, path).toBe('local')
      expect(caps.canWrite, path).toBe(true)
    }
  })

  it('falls back to the volume row when the portal toggle is OFF', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    gitPortal.on = false
    try {
      // With the portal off `resolve` routes nothing, so `.git/branches/` is a
      // plain (usually absent) directory on the parent drive.
      const caps = capabilitiesForPane('root', '/Users/me/repo/.git/branches/main')
      expect(caps.kind).toBe('local')
      expect(caps.canWrite).toBe(true)
    } finally {
      gitPortal.on = true
    }
  })

  it('a nested zip inside a read-only tar stays read-only (the outer format governs)', () => {
    volumes.list = [vol({ id: 'root', fsType: 'apfs', category: 'main_volume' })]
    // Leftmost archive component wins: `foo.tar` is the boundary, `bar.zip` is a
    // plain inner entry — so the pane is read-only, not writable.
    expect(capabilitiesForPane('root', '/x/foo.tar/bar.zip/y').canWrite).toBe(false)
  })
})

describe('paneRowsAreOsVisible — what the share sheet needs', () => {
  it('says yes for the two kinds whose rows are ordinary mounted files', () => {
    expect(paneRowsAreOsVisible('local')).toBe(true)
    expect(paneRowsAreOsVisible('smb')).toBe(true)
  })

  it('says yes for the search-results snapshot, whose rows are real files', () => {
    // The one place this parts company with `canOpenTerminalIn`: the snapshot pane
    // has no folder of its own, yet every row is a real path on disk.
    expect(paneRowsAreOsVisible('search-results')).toBe(true)
  })

  it('says no for every kind whose rows have no file behind them', () => {
    for (const kind of ['mtp', 'adb', 'network', 'archive', 'git-portal'] as const) {
      expect(paneRowsAreOsVisible(kind), kind).toBe(false)
    }
  })

  it('answers every kind in the union, so a new kind can`t be silently shareable', () => {
    const kinds: VolumeKind[] = ['local', 'smb', 'mtp', 'adb', 'network', 'search-results', 'archive', 'git-portal']
    for (const kind of kinds) {
      expect(typeof paneRowsAreOsVisible(kind), kind).toBe('boolean')
    }
  })
})

describe('rowIsOsVisible — the per-ROW share gate', () => {
  it('says yes for an ordinary row on a local volume', () => {
    volumes.list = [vol({ id: 'disk1', category: 'main_volume' })]
    expect(rowIsOsVisible('disk1', '/Users/me/photo.jpg')).toBe(true)
  })

  it('says no for a row STRICTLY inside an archive, on the very same volume', () => {
    // An archive pane keeps the parent drive's volumeId, so the volume alone says
    // "local" and only the row's path knows there's no file behind it.
    volumes.list = [vol({ id: 'disk1', category: 'main_volume' })]
    expect(rowIsOsVisible('disk1', '/Users/me/trip.zip/IMG_0001.jpg')).toBe(false)
  })

  it('still says yes for the archive FILE itself', () => {
    // The `.zip` is an ordinary file sitting in an ordinary folder; sharing it is
    // exactly what someone wants after compressing a selection.
    volumes.list = [vol({ id: 'disk1', category: 'main_volume' })]
    expect(rowIsOsVisible('disk1', '/Users/me/trip.zip')).toBe(true)
  })

  it('says no for a row inside the virtual git portal while the portal is on', () => {
    volumes.list = [vol({ id: 'disk1', category: 'main_volume' })]
    gitPortal.on = true
    expect(rowIsOsVisible('disk1', '/repo/.git/branches/main/src/main.rs')).toBe(false)
  })

  it('says yes for that same path once the portal is switched off', () => {
    // With the portal off nothing routes it, so the path is whatever is on disk.
    volumes.list = [vol({ id: 'disk1', category: 'main_volume' })]
    gitPortal.on = false
    expect(rowIsOsVisible('disk1', '/repo/.git/branches/main/src/main.rs')).toBe(true)
    gitPortal.on = true
  })

  it('says no on a phone, whatever the row looks like', () => {
    volumes.list = [vol({ id: 'mtp-1:1', category: 'mobile_device' })]
    expect(rowIsOsVisible('mtp-1:1', '/DCIM/IMG_0001.jpg')).toBe(false)
  })
})
