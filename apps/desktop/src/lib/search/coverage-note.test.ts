/**
 * The two pure decisions behind coverage honesty: what a run couldn't cover, and
 * whether the dialog should wait for an arena before asking at all.
 */

import { describe, it, expect } from 'vitest'
import type { SearchResult, SearchRunCoverage } from '$lib/tauri-commands'
import type { CoverageNote } from './coverage-note'
import { coverageNoteFrom, coverageNoteFromRun, isTargetIndexReady, offersFullDiskAccess } from './coverage-note'

function result(overrides: Partial<SearchResult> = {}): SearchResult {
  return { entries: [], totalCount: 0, ...overrides }
}

function runCoverage(overrides: Partial<SearchRunCoverage> = {}): SearchRunCoverage {
  return {
    walk: 'completed',
    kind: 'live',
    permissionDenied: [],
    declined: [],
    stillCovering: [],
    unresolvedScopes: [],
    abandonedGround: false,
    capped: false,
    targetVolumeId: 'root',
    ...overrides,
  }
}

describe('coverageNoteFromRun', () => {
  it('returns null for a walk that covered its whole frontier', () => {
    expect(coverageNoteFromRun(runCoverage())).toBeNull()
  })

  it('labels a walk that finished having given up on folders', () => {
    // The quiet way a run comes back short: every root covered, nobody stopped
    // it, and it still read less than the tree holds. Without a note here the
    // list reads as exhaustive (Accepted difference 9).
    const note = coverageNoteFromRun(runCoverage({ abandonedGround: true }))
    expect(note?.live?.abandonedGround).toBe(true)
    expect(note?.live?.walk).toBe('completed')
  })

  it('carries the flag alongside a cancel, which is a second reason to be short', () => {
    const note = coverageNoteFromRun(runCoverage({ walk: 'cancelled', abandonedGround: true }))
    expect(note?.live).toEqual({
      walk: 'cancelled',
      permissionDenied: [],
      declined: [],
      stillCovering: [],
      abandonedGround: true,
      abandonedLocations: 0,
    })
  })

  it('carries how many places it gave up on, so the note can say how much this is about', () => {
    const note = coverageNoteFromRun(runCoverage({ abandonedGround: true, abandonedLocations: 4 }))
    expect(note?.live?.abandonedLocations).toBe(4)
  })

  it('reads a backend that says nothing about places as none known', () => {
    // A run whose own walk gave up on ground it recorded no path for: something
    // was missed and nothing can say where, which the note handles in words.
    const note = coverageNoteFromRun(runCoverage({ abandonedGround: true, abandonedLocations: undefined }))
    expect(note?.live?.abandonedLocations).toBe(0)
  })
})

describe('coverageNoteFrom', () => {
  it('returns null for a run that covered everything asked of it', () => {
    expect(coverageNoteFrom(result())).toBeNull()
    expect(coverageNoteFrom(result({ uncoveredScopes: [], unresolvedScopes: [] }))).toBeNull()
  })

  it('carries an uncovered volume with the volume the backend routed to', () => {
    const note = coverageNoteFrom(result({ uncoveredScopes: ['/Volumes/naspi/photos'], targetVolumeId: 'smb-naspi' }))
    expect(note).toEqual({
      uncoveredScopes: ['/Volumes/naspi/photos'],
      unresolvedScopes: [],
      volumeId: 'smb-naspi',
    })
  })

  it('carries an unresolved path, the sibling gap on an indexed volume', () => {
    const note = coverageNoteFrom(result({ unresolvedScopes: ['/Users/x/gone'], targetVolumeId: 'root' }))
    expect(note?.unresolvedScopes).toEqual(['/Users/x/gone'])
    expect(note?.uncoveredScopes).toEqual([])
  })

  it('carries BOTH lists when both are filled', () => {
    // They're mutually exclusive today by construction, and a reader that assumed so
    // would go silent the day one run reports both. Each list is checked on its own.
    const note = coverageNoteFrom(
      result({ uncoveredScopes: ['/Volumes/usb'], unresolvedScopes: ['/Users/x/gone'], targetVolumeId: 'root' }),
    )
    expect(note?.uncoveredScopes).toEqual(['/Volumes/usb'])
    expect(note?.unresolvedScopes).toEqual(['/Users/x/gone'])
  })

  it('tolerates a backend that names no volume', () => {
    expect(coverageNoteFrom(result({ uncoveredScopes: ['/x'] }))?.volumeId).toBe('')
  })
})

describe('isTargetIndexReady', () => {
  const ready =
    (...ids: string[]) =>
    (id: string) =>
      ids.includes(id)

  it('runs when nothing is pending, even with no arena loaded anywhere', () => {
    // The machine that declined indexing: no arena, and none coming. Waiting here is
    // what made search inert; running gets an honest answer with its gap named.
    expect(isTargetIndexReady({ targetVolumeId: 'root', isVolumeReady: ready(), pendingVolumeId: null })).toBe(true)
  })

  it('waits while a pre-load for THIS volume is in flight', () => {
    expect(isTargetIndexReady({ targetVolumeId: 'root', isVolumeReady: ready(), pendingVolumeId: 'root' })).toBe(false)
  })

  it('does not wait for a pre-load of a volume the search will not touch', () => {
    expect(isTargetIndexReady({ targetVolumeId: 'smb-naspi', isVolumeReady: ready(), pendingVolumeId: 'root' })).toBe(
      true,
    )
  })

  it('runs once the target arena has landed, whatever else is pending', () => {
    expect(isTargetIndexReady({ targetVolumeId: 'root', isVolumeReady: ready('root'), pendingVolumeId: 'root' })).toBe(
      true,
    )
  })

  it('runs when the target is unknown, because waiting on a guess is how a search stops happening', () => {
    expect(isTargetIndexReady({ targetVolumeId: null, isVolumeReady: ready(), pendingVolumeId: 'root' })).toBe(true)
  })
})

describe('offersFullDiskAccess', () => {
  /** A note from a live run that met the ground `live` describes. */
  function note(live: Partial<NonNullable<CoverageNote['live']>>): CoverageNote {
    return {
      uncoveredScopes: [],
      unresolvedScopes: [],
      volumeId: 'root',
      live: {
        walk: 'completed',
        permissionDenied: [],
        declined: [],
        stillCovering: [],
        abandonedGround: false,
        abandonedLocations: 0,
        ...live,
      },
    }
  }

  const onMacWithout = { isMac: true, hasFullDiskAccess: false }

  it('offers the route when a folder was refused and Cmdr lacks the permission', () => {
    expect(offersFullDiskAccess({ note: note({ permissionDenied: ['/Users/me/Documents'] }), ...onMacWithout })).toBe(
      true,
    )
  })

  it('offers NOTHING for a snapshot folder, however many there are', () => {
    // The silent-failure case the typed cause exists for: these two are the same
    // shape on the wire and only one of them is a permission problem. Offering
    // Full Disk Access here sends the user to System Settings to fix nothing.
    expect(offersFullDiskAccess({ note: note({ declined: ['/Volumes/naspi/@eaDir'] }), ...onMacWithout })).toBe(false)
  })

  it('offers nothing once Cmdr already has full disk access', () => {
    // A refusal WITH the permission is a folder that belongs to someone else on
    // the machine, and the setup is already done.
    expect(
      offersFullDiskAccess({
        note: note({ permissionDenied: ['/Users/other/Documents'] }),
        isMac: true,
        hasFullDiskAccess: true,
      }),
    ).toBe(false)
  })

  it('offers nothing off macOS, where the permission does not exist', () => {
    expect(
      offersFullDiskAccess({
        note: note({ permissionDenied: ['/home/other'] }),
        isMac: false,
        hasFullDiskAccess: false,
      }),
    ).toBe(false)
  })

  it('offers nothing when there is no note at all', () => {
    expect(offersFullDiskAccess({ note: null, ...onMacWithout })).toBe(false)
  })
})
