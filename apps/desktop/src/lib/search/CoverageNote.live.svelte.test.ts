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
import { tString } from '$lib/intl/messages.svelte'
import type { CoverageNote as Note, LiveCoverage } from './coverage-note'

function liveNote(live: Partial<LiveCoverage>): Note {
  return {
    uncoveredScopes: [],
    unresolvedScopes: [],
    volumeId: 'root',
    live: { walk: 'completed', unreadable: [], stillCovering: [], abandonedGround: false, ...live },
  }
}

/** Mounts the strip and hands back its text with the markup's whitespace collapsed. */
function noteText(note: Note | null): string {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CoverageNote, {
    target,
    props: { note, driveName: 'Backups', isNetwork: false, onIndexDrive: null, onSilenceDrive: () => {} },
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

  it('says BOTH when a stopped walk had also given up on folders', () => {
    const text = noteText(liveNote({ walk: 'cancelled', abandonedGround: true }))
    expect(text).toContain(tString('search.coverage.walk.cancelled'))
    expect(text).toContain(tString('search.coverage.walk.abandoned'))
  })

  it('stays quiet about a walk that covered its ground', () => {
    expect(noteText(liveNote({}))).toBe('')
  })
})

describe('ground the run did not read', () => {
  it('lists unreadable folders and names both possible reasons, never one', () => {
    const text = noteText(liveNote({ unreadable: ['/Users/me/Documents', '/Volumes/naspi/@eaDir'] }))
    expect(text).toContain('/Users/me/Documents')
    expect(text).toContain('/Volumes/naspi/@eaDir')
    expect(text).toContain(tString('search.coverage.unreadableWhy'))
  })

  it('says another search is covering the rest, never that it is lost', () => {
    const text = noteText(liveNote({ stillCovering: ['/Users/me/Music'] }))
    expect(text).toContain('/Users/me/Music')
    expect(text).toContain(tString('search.coverage.stillCovering', { count: 1 }))
  })
})
