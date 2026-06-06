import { describe, it, expect } from 'vitest'
import { commands } from '$lib/commands/command-registry'
import { groupCommandsByScope, groupedScopes } from './keyboard-shortcuts-grouping'

describe('groupCommandsByScope', () => {
  it('renders every registry command in exactly one group', () => {
    const groups = groupCommandsByScope(commands)
    const grouped = groups.flatMap((g) => g.commands)

    // No command appears twice.
    const ids = grouped.map((c) => c.id)
    expect(new Set(ids).size).toBe(ids.length)

    // The union of grouped commands === all registry commands. This is the
    // regression guard: a compound-scope command (e.g. `file.quickLook` on
    // `'Main window/File list'`) that falls into no group would shrink this set
    // and become unrebindable through the UI.
    expect(new Set(ids)).toEqual(new Set(commands.map((c) => c.id)))
  })

  it('covers every scope present in the registry (no command silently dropped)', () => {
    const registryScopes = new Set(commands.map((c) => c.scope))
    for (const scope of registryScopes) {
      expect(groupedScopes).toContain(scope)
    }
  })

  it('drops empty groups and preserves the fixed scope order', () => {
    const groups = groupCommandsByScope(commands)
    expect(groups.every((g) => g.commands.length > 0)).toBe(true)

    const orderIndex = (scope: string) => groupedScopes.indexOf(scope as (typeof groupedScopes)[number])
    for (let i = 1; i < groups.length; i++) {
      expect(orderIndex(groups[i].scope)).toBeGreaterThan(orderIndex(groups[i - 1].scope))
    }
  })

  it('groups file.quickLook (a compound-scope command) so the Quick Look deep link lands', () => {
    const groups = groupCommandsByScope(commands)
    const quickLook = groups.flatMap((g) => g.commands).find((c) => c.id === 'file.quickLook')
    expect(quickLook).toBeDefined()
  })
})
