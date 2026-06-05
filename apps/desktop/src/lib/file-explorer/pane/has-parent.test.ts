/**
 * R3 T1: regression test for the round-2 P6 fix. Pins that
 * `computeHasParent` returns `false` for search-results panes, so
 * `selection.selectAll(hasParent, ...)` includes index 0. Without this
 * guard, ⌘A in a snapshot pane silently skipped the first row.
 */
import { describe, it, expect } from 'vitest'
import { computeHasParent } from './has-parent'
import { createSelectionState } from './selection-state.svelte'

describe('computeHasParent (R3 T1)', () => {
  it('returns false when the kind has no parent row (snapshot), regardless of path', () => {
    expect(
      computeHasParent({
        hasParentRow: false,
        currentPath: 'search-results://sr-1',
        effectiveVolumeRoot: '/',
      }),
    ).toBe(false)
    // Even when the synthetic path happens to "look like" a volume root.
    expect(
      computeHasParent({
        hasParentRow: false,
        currentPath: 'search-results://sr-42',
        effectiveVolumeRoot: 'search-results://sr-42',
      }),
    ).toBe(false)
  })

  it('returns false at the filesystem root', () => {
    expect(
      computeHasParent({
        hasParentRow: true,
        currentPath: '/',
        effectiveVolumeRoot: '/',
      }),
    ).toBe(false)
  })

  it('returns false at the volume root (non-/ volume)', () => {
    expect(
      computeHasParent({
        hasParentRow: true,
        currentPath: '/Volumes/External',
        effectiveVolumeRoot: '/Volumes/External',
      }),
    ).toBe(false)
  })

  it('returns true inside a folder on the volume', () => {
    expect(
      computeHasParent({
        hasParentRow: true,
        currentPath: '/Users/me/projects',
        effectiveVolumeRoot: '/',
      }),
    ).toBe(true)
  })
})

/**
 * R3 T1: pair the pure helper above with an actual `selectAll` invocation
 * to guarantee the integration works. When `hasParent` is false the
 * `selection.selectAll` includes index 0; this is the regression we're
 * pinning against round 1's "I/O-only" tests (no FilePane mount).
 */
describe('selectAll integration with computeHasParent (R3 T1)', () => {
  it('snapshot pane selectAll covers index 0 (ranges from 0..count-1)', () => {
    const sel = createSelectionState()
    const hasParent = computeHasParent({
      hasParentRow: false,
      currentPath: 'search-results://sr-1',
      effectiveVolumeRoot: '/',
    })
    expect(hasParent).toBe(false)
    sel.selectAll(hasParent, 5)
    // All five entries selected, including the all-important index 0.
    expect(sel.getSelectedIndices()).toEqual([0, 1, 2, 3, 4])
  })

  it('non-snapshot pane selectAll skips index 0 (the `..` row)', () => {
    const sel = createSelectionState()
    const hasParent = computeHasParent({
      hasParentRow: true,
      currentPath: '/Users/me',
      effectiveVolumeRoot: '/',
    })
    expect(hasParent).toBe(true)
    sel.selectAll(hasParent, 5)
    expect(sel.getSelectedIndices()).toEqual([1, 2, 3, 4])
  })
})
