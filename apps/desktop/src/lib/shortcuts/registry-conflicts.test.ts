/**
 * No two DEFAULT shortcuts may claim the same combo in overlapping scopes.
 *
 * This is the check that makes adding a shortcut safe. `conflict-detector.ts` already
 * knows how to answer "do these two clash?" (same combo AND overlapping scopes via
 * `scope-hierarchy.ts`), and the Settings editor warns a user who creates a clash by
 * hand — but nothing walked the SHIPPED defaults, so a new registry entry could quietly
 * shadow an existing binding for everyone. At runtime the dispatch map keeps exactly one
 * winner per combo, so the loser just silently stops working.
 *
 * A deliberate clash is fine ("Keep both" is a valid user choice), but it has to be
 * written down in `allowedConflicts` below with a reason, not discovered later.
 */

import { describe, it, expect, vi } from 'vitest'

// The registry gates several commands on `isMacOS()` AT MODULE LOAD (`file.quickLook`,
// the Finder-parity ⌘I, the cloud commands), so a `beforeAll` spy is too late — the
// arrays are already `[]` by then and those combos would go unchecked. Stub the UA from
// a `vi.hoisted` block, which runs before the imports, so this walks the real macOS
// default set we ship.
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

import { commands } from '$lib/commands/command-registry'
import { findConflictsForShortcut, getAllConflicts } from './conflict-detector'
import { getDefaultShortcuts } from './shortcuts-store'

/**
 * Clashes we've decided to keep, each with the reason it's safe. Adding a line here is
 * a deliberate product decision, not a way to quiet the test: every entry means one of
 * the two commands loses the combo at runtime.
 */
const allowedConflicts: { shortcut: string; commandIds: string[]; why: string }[] = []

/** `{ shortcut, commandIds }` rendered as a stable, greppable line. */
function describeConflict(conflict: { shortcut: string; commandIds: string[] }): string {
  return `${conflict.shortcut} → ${[...conflict.commandIds].sort().join(', ')}`
}

describe('registry default shortcuts', () => {
  it('has no two commands claiming one combo in overlapping scopes', () => {
    const actual = getAllConflicts().map(describeConflict).sort()
    const allowed = allowedConflicts.map(describeConflict).sort()
    expect(actual).toEqual(allowed)
  })

  it('lists no stale entries in the allowlist', () => {
    // A resolved conflict must drop its allowlist line, or the list rots into a
    // record of problems we no longer have.
    const actual = new Set(getAllConflicts().map(describeConflict))
    const stale = allowedConflicts.map(describeConflict).filter((line) => !actual.has(line))
    expect(stale).toEqual([])
  })

  it('the detector really sees the registry (guard against a vacuous pass)', () => {
    // "No conflicts" only means something if the detector CAN find one. A registry that
    // failed to load, or a scope typo that empties every ancestry chain, would also
    // report zero. So ask it about a combo we know is claimed, in the claimant's own
    // scope: it must name that command.
    const claimant = commands.find((command) => getDefaultShortcuts(command.id).length > 0)
    if (claimant === undefined) throw new Error('registry has no default shortcuts at all')
    const shortcut = getDefaultShortcuts(claimant.id)[0]

    expect(findConflictsForShortcut(shortcut, claimant.scope).map((c) => c.id)).toContain(claimant.id)
    // …and an unclaimed combo finds nothing, so it isn't just returning everything.
    expect(findConflictsForShortcut('⌘⌃⌥⇧F13', 'App')).toEqual([])
  })
})
