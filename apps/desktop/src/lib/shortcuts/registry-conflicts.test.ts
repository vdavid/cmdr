/**
 * No two DEFAULT shortcuts may claim the same combo in overlapping scopes, on any platform.
 *
 * This is the check that makes adding a shortcut safe. `conflict-detector.ts` already
 * knows how to answer "do these two clash?" (same combo AND overlapping scopes via
 * `scope-hierarchy.ts`), and the Settings editor warns a user who creates a clash by
 * hand — but nothing walked the SHIPPED defaults, so a new registry entry could quietly
 * shadow an existing binding for everyone. At runtime the dispatch map keeps exactly one
 * winner per combo, so the loser just silently stops working.
 *
 * It runs over the real registry with no mocks, so it also guards that compound-scope
 * commands (`Main window/File list`, `Main window/Brief mode`, …) take part in detection at
 * all. Each platform runs with what that OS can offer (`withCapabilitiesPossibleOn`): a
 * command the OS can't offer holds no default key there, so it claims nothing.
 *
 * A deliberate clash is fine ("Keep both" is a valid user choice), but it has to be
 * written down in `allowedConflicts` below with a reason, not discovered later.
 */

import { afterAll, beforeAll, describe, it, expect } from 'vitest'
import type { Platform } from '#lib/platform/index.js'
import { withCapabilitiesPossibleOn, withPlatform } from '#lib/platform/test-platform.js'
import { commands } from '#lib/commands/command-registry.js'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import { findConflictsForShortcut, getAllConflicts } from './conflict-detector'
import { getDefaultShortcuts } from './shortcuts-store'

interface AllowedConflict {
  shortcut: string
  commandIds: string[]
  why: string
}

/**
 * Show favorites' `⌃D`, Duplicate's `⌘D`, and the error screen's `⌘D` details toggle are all
 * `Ctrl+D` off macOS. The detector reports one line per combo, naming every claimant.
 */
const CTRL_D_OFF_MACOS: AllowedConflict = {
  shortcut: 'Ctrl+D',
  commandIds: ['errorPane.toggleTechnicalDetails', 'favorites.open', 'file.duplicate'],
  why: 'Moves in the keymap work (#1102), which gives Windows and Linux their own keys.',
}

/**
 * Clashes we've decided to keep, per platform, each with the reason it's safe. Adding a line
 * here is a deliberate product decision, not a way to quiet the test: every entry means one of
 * the commands loses the combo at runtime.
 */
const allowedConflicts: Record<Platform, AllowedConflict[]> = {
  macos: [],
  windows: [CTRL_D_OFF_MACOS],
  linux: [CTRL_D_OFF_MACOS],
}

/** `{ shortcut, commandIds }` rendered as a stable, greppable line. */
function describeConflict(conflict: { shortcut: string; commandIds: string[] }): string {
  return `${conflict.shortcut} → ${[...conflict.commandIds].sort().join(', ')}`
}

describe.each(PLATFORMS)('registry default shortcuts on %s', (platform) => {
  const undos: (() => void)[] = []

  beforeAll(() => {
    undos.push(withPlatform(platform), withCapabilitiesPossibleOn(platform))
  })

  afterAll(() => {
    for (const undo of undos.reverse()) undo()
  })

  it('has no two commands claiming one combo in overlapping scopes', () => {
    const actual = getAllConflicts().map(describeConflict).sort()
    const allowed = allowedConflicts[platform].map(describeConflict).sort()
    expect(actual).toEqual(allowed)
  })

  it('lists no stale entries in the allowlist', () => {
    // A resolved conflict must drop its allowlist line, or the list rots into a
    // record of problems we no longer have.
    const actual = new Set(getAllConflicts().map(describeConflict))
    const stale = allowedConflicts[platform].map(describeConflict).filter((line) => !actual.has(line))
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
    expect(findConflictsForShortcut(platform === 'macos' ? '⌘⌃⌥⇧F13' : 'Ctrl+Alt+Shift+F13', 'App')).toEqual([])
  })
})
