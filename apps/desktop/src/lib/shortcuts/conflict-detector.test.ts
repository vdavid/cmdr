/**
 * Tests for conflict detection logic in keyboard shortcuts.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { Command } from '$lib/commands/types'
import type { CommandScope } from './scope-hierarchy'

// Shared test state — mock factory closures capture these references
const customOverrides = new Map<string, string[]>()

const testCommands: Command[] = []

vi.mock('$lib/commands/command-registry', () => ({
  get commands() {
    return testCommands
  },
}))

vi.mock('./shortcuts-store', () => ({
  getEffectiveShortcuts: vi.fn(),
}))

import { getEffectiveShortcuts } from './shortcuts-store'
import {
  findConflictsForShortcut,
  getAllConflicts,
  getConflictCount,
  getConflictingCommandIds,
} from './conflict-detector'

function setupEffectiveShortcuts() {
  vi.mocked(getEffectiveShortcuts).mockImplementation((commandId: string) => {
    const override = customOverrides.get(commandId)
    if (override) return [...override]
    const cmd = testCommands.find((c) => c.id === commandId)
    return [...(cmd?.shortcuts ?? [])]
  })
}

function setCommands(...cmds: Command[]) {
  testCommands.length = 0
  testCommands.push(...cmds)
}

function makeCommand(partial: Omit<Partial<Command>, 'scope'> & { id: string; scope?: CommandScope }): Command {
  return {
    name: partial.id,
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    ...partial,
  } as Command
}

describe('conflict-detector', () => {
  beforeEach(() => {
    customOverrides.clear()
    testCommands.length = 0
    vi.clearAllMocks()
    setupEffectiveShortcuts()
  })

  // ========================================================================
  // findConflictsForShortcut
  // ========================================================================

  describe('findConflictsForShortcut', () => {
    it('returns empty array for an empty shortcut string', () => {
      setCommands(makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }))
      expect(findConflictsForShortcut('', 'App')).toEqual([])
    })

    it('returns commands sharing the same shortcut in overlapping scopes', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'Main window' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'File list' }),
      )
      const conflicts = findConflictsForShortcut('⌘N', 'File list')
      const ids = conflicts.map((c) => c.id)
      expect(ids).toContain('a')
      expect(ids).toContain('b')
    })

    it('excludes the command specified by excludeCommandId', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'App' }),
      )
      const conflicts = findConflictsForShortcut('⌘N', 'App', 'a')
      const ids = conflicts.map((c) => c.id)
      expect(ids).not.toContain('a')
      expect(ids).toContain('b')
    })

    it('returns empty array when no commands share the shortcut', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘B'], scope: 'App' }),
      )
      expect(findConflictsForShortcut('⌘Z', 'App')).toEqual([])
    })

    it('returns empty array when scopes do not overlap', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'About window' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'Settings window' }),
      )
      // About window and Settings window don't overlap
      const conflicts = findConflictsForShortcut('⌘N', 'About window')
      const ids = conflicts.map((c) => c.id)
      expect(ids).toContain('a')
      expect(ids).not.toContain('b')
    })

    it('ignores commands with empty shortcuts', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: [''], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'App' }),
      )
      const conflicts = findConflictsForShortcut('⌘N', 'App')
      const ids = conflicts.map((c) => c.id)
      expect(ids).toContain('b')
      expect(ids).not.toContain('a')
    })

    it('uses effective shortcuts (custom overrides)', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘B'], scope: 'App' }),
      )
      // Override b to use ⌘A
      customOverrides.set('b', ['⌘A'])

      const conflicts = findConflictsForShortcut('⌘A', 'App')
      const ids = conflicts.map((c) => c.id)
      expect(ids).toContain('a')
      expect(ids).toContain('b')
    })
  })

  // ========================================================================
  // getAllConflicts
  // ========================================================================

  describe('getAllConflicts', () => {
    it('returns empty array when there are no commands', () => {
      setCommands()
      expect(getAllConflicts()).toEqual([])
    })

    it('returns empty array for a single command', () => {
      setCommands(makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }))
      expect(getAllConflicts()).toEqual([])
    })

    it('returns empty array when commands use different shortcuts', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘B'], scope: 'App' }),
      )
      expect(getAllConflicts()).toEqual([])
    })

    it('detects a conflict between two commands with the same shortcut and overlapping scopes', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'Main window' }),
      )
      const conflicts = getAllConflicts()
      expect(conflicts).toHaveLength(1)
      expect(conflicts[0].shortcut).toBe('⌘N')
      expect(conflicts[0].commandIds).toContain('a')
      expect(conflicts[0].commandIds).toContain('b')
    })

    it('does not report a conflict when scopes do not overlap', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'About window' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'Settings window' }),
      )
      expect(getAllConflicts()).toEqual([])
    })

    it('deduplicates: reports each shortcut conflict once', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'c', shortcuts: ['⌘N'], scope: 'App' }),
      )
      const conflicts = getAllConflicts()
      const nConflicts = conflicts.filter((c) => c.shortcut === '⌘N')
      expect(nConflicts).toHaveLength(1)
      expect(nConflicts[0].commandIds).toHaveLength(3)
    })

    it('handles three commands sharing a shortcut with partial scope overlap', () => {
      // a: App (overlaps with everything)
      // b: About window (overlaps with App but not Settings window)
      // c: Settings window (overlaps with App but not About window)
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'About window' }),
        makeCommand({ id: 'c', shortcuts: ['⌘N'], scope: 'Settings window' }),
      )
      const conflicts = getAllConflicts()
      expect(conflicts).toHaveLength(1)
      // All three should be reported because each overlaps with at least one other
      expect(conflicts[0].commandIds).toContain('a')
      expect(conflicts[0].commandIds).toContain('b')
      expect(conflicts[0].commandIds).toContain('c')
    })

    it('reports multiple distinct shortcut conflicts', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'c', shortcuts: ['⌘B'], scope: 'App' }),
        makeCommand({ id: 'd', shortcuts: ['⌘B'], scope: 'App' }),
      )
      const conflicts = getAllConflicts()
      expect(conflicts).toHaveLength(2)
      const shortcuts = conflicts.map((c) => c.shortcut).sort()
      expect(shortcuts).toEqual(['⌘A', '⌘B'])
    })
  })

  // ========================================================================
  // getConflictCount
  // ========================================================================

  describe('getConflictCount', () => {
    it('returns 0 when there are no conflicts', () => {
      setCommands(makeCommand({ id: 'a', shortcuts: ['⌘A'], scope: 'App' }))
      expect(getConflictCount()).toBe(0)
    })

    it('returns the number of unique command IDs involved in conflicts', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'c', shortcuts: ['⌘X'], scope: 'App' }),
      )
      // Only a and b conflict; c is uninvolved
      expect(getConflictCount()).toBe(2)
    })

    it('deduplicates command IDs across multiple conflicts', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘A', '⌘B'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘A'], scope: 'App' }),
        makeCommand({ id: 'c', shortcuts: ['⌘B'], scope: 'App' }),
      )
      // a conflicts on both ⌘A (with b) and ⌘B (with c), but a is counted once
      expect(getConflictCount()).toBe(3)
    })
  })

  // ========================================================================
  // getConflictingCommandIds
  // ========================================================================

  describe('getConflictingCommandIds', () => {
    it('returns an empty set when there are no conflicts', () => {
      setCommands()
      expect(getConflictingCommandIds().size).toBe(0)
    })

    it('returns the set of all conflicting command IDs', () => {
      setCommands(
        makeCommand({ id: 'a', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'b', shortcuts: ['⌘N'], scope: 'App' }),
        makeCommand({ id: 'c', shortcuts: ['⌘X'], scope: 'App' }),
      )
      const ids = getConflictingCommandIds()
      expect(ids.has('a')).toBe(true)
      expect(ids.has('b')).toBe(true)
      expect(ids.has('c')).toBe(false)
    })
  })
})
