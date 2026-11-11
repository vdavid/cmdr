/**
 * What the coverage note says about a LIVE run: the three ways a walk leaves the list
 * short, and the two lists of ground it didn't read.
 *
 * Each branch is a distinct sentence because each is a distinct fact, and getting the
 * mapping wrong is invisible — the strip renders, it just tells the user the wrong
 * story about their results. The abandoned-ground line is the one that most needs
 * pinning: it's true ALONGSIDE the walk's ending rather than instead of it, so a
 * cancelled walk that also gave up on folders has to say both.
 */

import { describe, it, expect } from 'vitest'
import { mount, flushSync } from 'svelte'
import CoverageNote from './CoverageNote.svelte'
import { tString } from '#lib/intl/messages.svelte.js'
import type { CoverageNote as Note, LiveCoverage } from './coverage-note'

function liveNote(live: Partial<LiveCoverage>): Note {
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

/** The count params every one-folder line takes. */
const one = { count: 1, countText: '1' }

/** Mounts the strip and hands back its text with the markup's whitespace collapsed. */
function noteText(note: Note | null, onGrantFullDiskAccess: (() => void) | null = null, isIndexing = false): string {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CoverageNote, {
    target,
    props: {
      note,
      driveName: 'Backups',
      isNetwork: false,
      isIndexing,
      canBeIndexed: true,
      onIndexDrive: null,
      onSilenceDrive: () => {},
      onGrantFullDiskAccess,
    },
  })
  flushSync()
  const text = (target.querySelector('.coverage-note')?.textContent ?? '').replace(/\s+/g, ' ').trim()
  target.remove()
  return text
}

describe('a live run that came back short', () => {
  it('says the user stopped it', () => {
    expect(noteText(liveNote({ walk: 'cancelled' }))).toContain(tString('search.coverage.walk.cancelled'))
  })

  it('names the drive when the walk stopped on its own', () => {
    expect(noteText(liveNote({ walk: 'interrupted' }))).toContain(
      tString('search.coverage.walk.interrupted', { drive: 'Backups' }),
    )
  })

  it('admits the folders it gave up on, even on a walk that reached the end', () => {
    // The quiet third way short (Accepted difference 9): `walk: completed` and the
    // list is still a lower bound. Without this line it reads as exhaustive.
    const text = noteText(liveNote({ walk: 'completed', abandonedGround: true }))
    expect(text).toContain(tString('search.coverage.walk.abandoned'))
  })

  it('says how many PLACES it gave up on when it can, so a short list has a size', () => {
    const text = noteText(liveNote({ walk: 'completed', abandonedGround: true, abandonedLocations: 3 }))
    expect(text).toContain(tString('search.coverage.walk.abandonedCount', { count: 3, countText: '3' }))
    // ❌ Never the count-free wording alongside the counted one.
    expect(text).not.toContain(tString('search.coverage.walk.abandoned'))
  })

  it('says BOTH when a stopped walk had also given up on folders', () => {
    const text = noteText(liveNote({ walk: 'cancelled', abandonedGround: true }))
    expect(text).toContain(tString('search.coverage.walk.cancelled'))
    expect(text).toContain(tString('search.coverage.walk.abandoned'))
  })

  it('puts the gave-up-on footnote UNDER the two lists it belongs beside', () => {
    // The family reads in order of what it asks of you: refused (grant access),
    // declined (nothing to grant), gave up (nothing at all, Cmdr retries). A
    // footnote over the top of its own lists reads as the headline instead.
    const text = noteText(
      liveNote({
        permissionDenied: ['/Users/me/Documents'],
        declined: ['/Volumes/naspi/@eaDir'],
        abandonedGround: true,
        abandonedLocations: 2,
      }),
    )
    const denied = text.indexOf(tString('search.coverage.line.denied', one))
    const gaveUp = text.indexOf(tString('search.coverage.walk.abandonedCount', { count: 2, countText: '2' }))
    expect(denied).toBeGreaterThanOrEqual(0)
    expect(gaveUp).toBeGreaterThan(denied)
  })

  it('names a folder it could not open, and offers no Enter it already pressed', () => {
    const folder = '/Volumes/naspi/papers/Rymdskottkärra AB'
    const text = noteText(liveNote({ failedRoots: [folder] }))
    expect(text).toContain(tString('search.coverage.line.failedRoots', one))
    expect(text).toContain(folder)
    expect(text).not.toContain(tString('search.coverage.pressEnter'))
  })

  it('stays quiet about a walk that covered its ground', () => {
    expect(noteText(liveNote({}))).toBe('')
  })
})

describe('ground the run did not read', () => {
  it('says a refused folder was refused, and offers the way out when there is one', () => {
    const note = liveNote({ permissionDenied: ['/Users/me/Documents'] })
    const withoutRoute = noteText(note)
    expect(withoutRoute).toContain('/Users/me/Documents')
    expect(withoutRoute).toContain(tString('search.coverage.line.denied', one))
    // No route: the fact is still stated, the offer isn't. A gap with no way out
    // is still a gap.
    expect(withoutRoute).not.toContain(tString('search.coverage.setUpFullDiskAccess'))

    const withRoute = noteText(note, () => {})
    expect(withRoute).toContain(tString('search.coverage.deniedFullDiskAccess'))
    expect(withRoute).toContain(tString('search.coverage.setUpFullDiskAccess'))
  })

  it('says a snapshot folder is one Cmdr skips, and NEVER offers a permission for it', () => {
    // The whole point of the typed cause: no permission opens a snapshot tree, so
    // offering one here would be advice that does nothing.
    const text = noteText(liveNote({ declined: ['/Volumes/naspi/@eaDir'] }), () => {})
    expect(text).toContain('/Volumes/naspi/@eaDir')
    expect(text).toContain(tString('search.coverage.line.declined', one))
    expect(text).not.toContain(tString('search.coverage.setUpFullDiskAccess'))
  })

  it('keeps the two apart when a run met both', () => {
    const text = noteText(liveNote({ permissionDenied: ['/Users/me/Documents'], declined: ['/Volumes/naspi/@eaDir'] }))
    expect(text).toContain(tString('search.coverage.line.denied', one))
    expect(text).toContain(tString('search.coverage.line.declined', one))
  })

  it('says another search is covering the rest, never that it is lost', () => {
    const text = noteText(liveNote({ stillCovering: ['/Users/me/Music'] }))
    expect(text).toContain('/Users/me/Music')
    expect(text).toContain(tString('search.coverage.line.stillCovering', one))
  })
})

describe('a drive whose first index is still running', () => {
  /** An index-only run over a drive the walker hasn't reached yet. */
  const uncovered: Note = {
    uncoveredScopes: ['/Users/me/Projects'],
    unresolvedScopes: [],
    volumeId: 'root',
  }

  it('says the indexing is under way rather than that the drive is unindexed', () => {
    const text = noteText(uncovered, null, true)
    expect(text).toContain(tString('search.coverage.line.uncovered.indexing', { ...one, drive: 'Backups' }))
    // ❌ Never the "hasn't indexed it yet" wording: it reads as nothing happening.
    expect(text).not.toContain(tString('search.coverage.line.uncovered.local', { ...one, drive: 'Backups' }))
  })

  it('keeps the plain wording when nothing is indexing', () => {
    expect(noteText(uncovered)).toContain(tString('search.coverage.line.uncovered.local', { ...one, drive: 'Backups' }))
  })
})

describe('each list as one line', () => {
  /** Mounts the strip and hands back its root, for the line's icon and copy button. */
  function mountNote(note: Note, onGrantFullDiskAccess: (() => void) | null = null): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CoverageNote, {
      target,
      props: {
        note,
        driveName: 'Backups',
        isNetwork: false,
        isIndexing: false,
        canBeIndexed: true,
        onIndexDrive: null,
        onSilenceDrive: () => {},
        onGrantFullDiskAccess,
      },
    })
    flushSync()
    return target
  }

  const line = (root: HTMLElement, kind: string): HTMLElement | null => root.querySelector(`[data-kind="${kind}"]`)
  const isWarning = (el: HTMLElement | null): boolean =>
    el?.querySelector('.info-tip')?.classList.contains('warning') ?? false

  it('says a folder the system keeps closed is closed, and never offers a permission for it', () => {
    const root = mountNote(liveNote({ offLimits: ['/private/var/root'] }), () => {})
    const text = root.textContent ?? ''
    expect(text).toContain(tString('search.coverage.line.offLimits', one))
    expect(text).toContain('/private/var/root')
    expect(text).not.toContain(tString('search.coverage.setUpFullDiskAccess'))
    expect(isWarning(line(root, 'offLimits'))).toBe(false)
    root.remove()
  })

  it('warns over a refusal only while Full Disk Access would open it', () => {
    const note = liveNote({ permissionDenied: ['/Users/me/Library/Mail'] })
    const offered = mountNote(note, () => {})
    expect(isWarning(line(offered, 'denied'))).toBe(true)
    offered.remove()
    const notOffered = mountNote(note)
    expect(isWarning(line(notOffered, 'denied'))).toBe(false)
    notOffered.remove()
  })

  it('numbers at most 15 folders in the tooltip and says how many more there are', () => {
    const paths = Array.from({ length: 18 }, (_, i) => `/System/Library/AssetsV2/f${String(i)}`)
    const root = mountNote(liveNote({ offLimits: paths }))
    const el = line(root, 'offLimits')
    expect(el?.querySelectorAll('ol li')).toHaveLength(15)
    expect(el?.textContent).toContain(tString('search.coverage.list.more', { count: 3, countText: '3' }))
    expect(el?.textContent).not.toContain(paths[15])
    root.remove()
  })

  it('gives every line its copy button and a keyboard-reachable folder list', () => {
    const root = mountNote(liveNote({ declined: ['/Volumes/naspi/@eaDir'], stillCovering: ['/Users/me/Music'] }))
    for (const kind of ['declined', 'stillCovering']) {
      const el = line(root, kind)
      expect(el?.querySelector('button.info-tip')?.getAttribute('aria-label')).toBe(
        tString('search.coverage.list.show'),
      )
      expect(el?.querySelector('button.copy-button')?.getAttribute('aria-label')).toBe(
        tString('search.coverage.list.copy'),
      )
    }
    root.remove()
  })
})
