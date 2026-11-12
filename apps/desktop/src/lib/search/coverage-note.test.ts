/**
 * The two pure decisions behind coverage honesty: what a run couldn't cover, and
 * whether the dialog should wait for an arena before asking at all.
 */

import { describe, it, expect } from 'vitest'
import type { SearchResult, SearchRunCoverage } from '#lib/tauri-commands/index.js'
import type { CoverageNote } from './coverage-note'
import {
  TOOLTIP_FOLDER_CAP,
  coverageLines,
  coverageNoteFrom,
  coverageNoteFromRun,
  foldersAsText,
  isTargetIndexReady,
  offersFullDiskAccess,
  tooltipFolders,
} from './coverage-note'

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

  it('never goes quiet over a folder the walk could not open', () => {
    // Regression: a NAS folder whose name the server didn't recognize came back as
    // a completed walk with nothing in it, and the dialog said "No files match".
    const folder = '/Volumes/testshare/documents/Testföretag AB'
    const note = coverageNoteFromRun(runCoverage({ failedRoots: [folder] }))
    expect(note?.live?.failedRoots).toEqual([folder])
  })

  it('reads a backend that sends no failed folders as none', () => {
    expect(
      coverageNoteFromRun(runCoverage({ abandonedGround: true, failedRoots: undefined }))?.live?.failedRoots,
    ).toEqual([])
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
      offLimits: [],
      declined: [],
      stillCovering: [],
      abandonedGround: true,
      abandonedLocations: 0,
      failedRoots: [],
    })
  })

  it('keeps folders nothing can open apart from the ones Full Disk Access opens', () => {
    const note = coverageNoteFromRun(
      runCoverage({ permissionDenied: ['/Users/me/Library/Mail'], offLimits: ['/private/var/root'] }),
    )
    expect(note?.live?.permissionDenied).toEqual(['/Users/me/Library/Mail'])
    expect(note?.live?.offLimits).toEqual(['/private/var/root'])
  })

  it('never goes quiet over a folder nothing can open', () => {
    // They stay visible: David's call, so an empty list over `/` isn't read as exhaustive.
    expect(coverageNoteFromRun(runCoverage({ offLimits: ['/usr/sbin/authserver'] }))?.live?.offLimits).toEqual([
      '/usr/sbin/authserver',
    ])
  })

  it('reads a backend that sends no off-limits folders as none', () => {
    expect(coverageNoteFromRun(runCoverage({ abandonedGround: true }))?.live?.offLimits).toEqual([])
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
    const note = coverageNoteFrom(
      result({ uncoveredScopes: ['/Volumes/testshare/photos'], targetVolumeId: 'smb-testshare' }),
    )
    expect(note).toEqual({
      uncoveredScopes: ['/Volumes/testshare/photos'],
      unresolvedScopes: [],
      volumeId: 'smb-testshare',
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
    expect(
      isTargetIndexReady({ targetVolumeId: 'smb-testshare', isVolumeReady: ready(), pendingVolumeId: 'root' }),
    ).toBe(true)
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
        offLimits: [],
        declined: [],
        stillCovering: [],
        abandonedGround: false,
        abandonedLocations: 0,
        failedRoots: [],
        ...live,
      },
    }
  }

  it('offers NOTHING for a folder the system keeps closed', () => {
    // SIP and root-only folders refuse WITH Full Disk Access too.
    expect(offersFullDiskAccess({ note: note({ offLimits: ['/private/var/root'] }), ...onMacWithout })).toBe(false)
  })

  const onMacWithout = { offersFda: true, hasFullDiskAccess: false }

  it('offers the route when a folder was refused and Cmdr lacks the permission', () => {
    expect(offersFullDiskAccess({ note: note({ permissionDenied: ['/Users/me/Documents'] }), ...onMacWithout })).toBe(
      true,
    )
  })

  it('offers NOTHING for a snapshot folder, however many there are', () => {
    // The silent-failure case the typed cause exists for: these two are the same
    // shape on the wire and only one of them is a permission problem. Offering
    // Full Disk Access here sends the user to System Settings to fix nothing.
    expect(offersFullDiskAccess({ note: note({ declined: ['/Volumes/testshare/@eaDir'] }), ...onMacWithout })).toBe(
      false,
    )
  })

  it('offers nothing once Cmdr already has full disk access', () => {
    // A refusal WITH the permission is a folder that belongs to someone else on
    // the machine, and the setup is already done.
    expect(
      offersFullDiskAccess({
        note: note({ permissionDenied: ['/Users/other/Documents'] }),
        offersFda: true,
        hasFullDiskAccess: true,
      }),
    ).toBe(false)
  })

  it('offers nothing off macOS, where the permission does not exist', () => {
    expect(
      offersFullDiskAccess({
        note: note({ permissionDenied: ['/home/other'] }),
        offersFda: false,
        hasFullDiskAccess: false,
      }),
    ).toBe(false)
  })

  it('offers nothing when there is no note at all', () => {
    expect(offersFullDiskAccess({ note: null, ...onMacWithout })).toBe(false)
  })
})

describe('coverageLines', () => {
  const noOffers = { fullDiskAccess: false, indexDrive: false, pressEnter: false }
  const live = (over: Partial<NonNullable<CoverageNote['live']>>): CoverageNote => ({
    uncoveredScopes: [],
    unresolvedScopes: [],
    volumeId: 'root',
    live: {
      walk: 'completed',
      permissionDenied: [],
      offLimits: [],
      declined: [],
      stillCovering: [],
      abandonedGround: false,
      abandonedLocations: 0,
      failedRoots: [],
      ...over,
    },
  })

  it('gives every filled list its own line, in the order the strip reads', () => {
    const lines = coverageLines(
      live({
        failedRoots: ['/a'],
        permissionDenied: ['/b'],
        offLimits: ['/c'],
        declined: ['/d'],
        stillCovering: ['/e'],
      }),
      noOffers,
    )
    expect(lines.unread.map((l) => l.kind)).toEqual(['failedRoots', 'denied', 'offLimits', 'declined'])
    expect(lines.unsearched.map((l) => l.kind)).toEqual(['stillCovering'])
  })

  it('leaves an empty list out entirely', () => {
    expect(coverageLines(live({}), noOffers)).toEqual({ unread: [], unsearched: [] })
  })

  it('marks a refusal actionable only while Full Disk Access is on offer', () => {
    const note = live({ permissionDenied: ['/Users/me/Library/Mail'] })
    expect(coverageLines(note, noOffers).unread[0].actionable).toBe(false)
    expect(coverageLines(note, { ...noOffers, fullDiskAccess: true }).unread[0].actionable).toBe(true)
  })

  it('never marks a folder nothing can open actionable, whatever is on offer', () => {
    const all = { fullDiskAccess: true, indexDrive: true, pressEnter: true }
    const lines = coverageLines(live({ offLimits: ['/c'], declined: ['/d'], stillCovering: ['/e'] }), all)
    expect([...lines.unread, ...lines.unsearched].some((l) => l.actionable)).toBe(false)
  })

  it('marks an index-only gap actionable when indexing or Enter would close it', () => {
    const note: CoverageNote = { uncoveredScopes: ['/Volumes/usb'], unresolvedScopes: ['/x'], volumeId: 'usb' }
    expect(coverageLines(note, noOffers).unsearched.map((l) => l.actionable)).toEqual([false, false])
    expect(coverageLines(note, { ...noOffers, indexDrive: true }).unsearched.map((l) => l.actionable)).toEqual([
      true,
      false,
    ])
    expect(coverageLines(note, { ...noOffers, pressEnter: true }).unsearched.map((l) => l.actionable)).toEqual([
      true,
      true,
    ])
  })
})

describe('tooltipFolders', () => {
  it('shows a short list whole', () => {
    expect(tooltipFolders(['/a', '/b'])).toEqual({ shown: ['/a', '/b'], more: 0 })
  })

  it('caps a long list and counts what it left out, so the tooltip never scrolls', () => {
    const paths = Array.from({ length: 18 }, (_, i) => `/f${String(i)}`)
    const { shown, more } = tooltipFolders(paths)
    expect(shown).toHaveLength(TOOLTIP_FOLDER_CAP)
    expect(shown[0]).toBe('/f0')
    expect(more).toBe(18 - TOOLTIP_FOLDER_CAP)
  })
})

describe('foldersAsText', () => {
  it('copies every folder, one per line, past the tooltip cap too', () => {
    const paths = Array.from({ length: 20 }, (_, i) => `/f${String(i)}`)
    expect(foldersAsText(paths).split('\n')).toEqual(paths)
  })
})
