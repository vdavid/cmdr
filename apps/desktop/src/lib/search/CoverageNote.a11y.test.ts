/**
 * Tier 3 a11y tests for `CoverageNote.svelte`: the strip that says why a search came
 * back empty (message, the skipped scope paths, and the per-drive offer) must have no
 * axe violations, in both the offered and the silenced shape.
 */
import { describe, it, expect } from 'vitest'
import { mount, flushSync } from 'svelte'
import CoverageNote from './CoverageNote.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { CoverageNote as Note } from './coverage-note'

function mountNote(note: Note | null, onIndexDrive: (() => void) | null) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CoverageNote, {
    target,
    props: {
      note,
      driveName: 'Naspolya',
      isNetwork: true,
      isIndexing: false,
      onIndexDrive,
      onSilenceDrive: () => {},
    },
  })
  flushSync()
  return target
}

const UNCOVERED: Note = {
  uncoveredScopes: ['/Volumes/naspi/photos'],
  unresolvedScopes: [],
  volumeId: 'smb-naspi',
}

describe('CoverageNote a11y', () => {
  it('the uncovered note with its offer has no violations', async () => {
    const target = mountNote(UNCOVERED, () => {})
    expect(target.querySelector('.coverage-note button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the note without an offer (a silenced drive) has no violations', async () => {
    const target = mountNote(UNCOVERED, null)
    expect(target.querySelector('.coverage-note button')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('an unresolved-path note has no violations', async () => {
    const target = mountNote(
      { uncoveredScopes: [], unresolvedScopes: ['/Users/test/gone', '/Users/test/also-gone'], volumeId: 'root' },
      null,
    )
    await expectNoA11yViolations(target)
  })

  it('stays mounted with nothing to say, so the live region survives to announce the next run', async () => {
    const target = mountNote(null, null)
    const strip = target.querySelector('.coverage-note')
    expect(strip).not.toBeNull()
    expect(strip?.getAttribute('role')).toBe('status')
    expect(strip?.textContent.trim()).toBe('')
    await expectNoA11yViolations(target)
  })
})
