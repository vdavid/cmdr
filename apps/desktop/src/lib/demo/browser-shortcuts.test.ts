/**
 * The demo's browser-safe shortcut map (`browser-shortcuts.ts`) has to hold up as a real
 * customization: every id a command, every combo one a keypress produces, no new clash, and none
 * of the combos the browser takes before the page sees them.
 */
import { describe, expect, it, vi } from 'vitest'

// The registry gates some defaults on `isMacOS()` at module load, so pin macOS before the
// imports, as `registry-conflicts.test.ts` does: this walks the macOS defaults the demo shows.
vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))

import { COMMAND_IDS } from '#lib/commands/command-ids.js'
import { commands } from '#lib/commands/command-registry.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { scopesOverlap } from '#lib/shortcuts/scope-hierarchy.js'
import { browserSafeShortcuts } from './browser-shortcuts.js'

const remapped = Object.entries<string[]>(browserSafeShortcuts)

/** The combos a browser keeps for itself on macOS: new tab or window, close tab, reopen tab, switch to tab N. */
const browserReserved = new Set(['⌘T', '⌘W', '⌘N', '⌘⇧T', ...Array.from({ length: 9 }, (_, i) => `⌘${String(i + 1)}`)])

/** The keydown a stored combo stands for (the subset of the vocabulary this map uses: letters, digits, F-keys). */
function keydownFor(combo: string): KeyboardEvent {
  const event = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, key: '', code: '' }
  let rest = combo
  const flags: Record<string, keyof typeof event> = { '⌘': 'metaKey', '⌃': 'ctrlKey', '⌥': 'altKey', '⇧': 'shiftKey' }
  while (rest.length > 0 && rest[0] in flags) {
    ;(event as Record<string, unknown>)[flags[rest[0]]] = true
    rest = rest.slice(1)
  }
  event.key = rest.length === 1 ? rest.toLowerCase() : rest
  return event as unknown as KeyboardEvent
}

/** Every command's combos as the demo seeds them: the map's where it remaps, the registry's otherwise. */
const effective = commands.map((command) => ({
  id: command.id,
  scope: command.scope,
  shortcuts: (browserSafeShortcuts as Record<string, string[] | undefined>)[command.id] ?? command.shortcuts,
}))

describe('browser-safe shortcut map', () => {
  it('remaps only real commands', () => {
    const known = new Set<string>(COMMAND_IDS)
    expect(remapped.map(([id]) => id).filter((id) => !known.has(id))).toEqual([])
  })

  it.each(remapped.flatMap(([id, combos]) => combos.map((combo) => [id, combo])))(
    '%s: %s is reachable by a real keypress',
    (_id, combo) => {
      expect(formatKeyCombo(keydownFor(combo))).toBe(combo)
    },
  )

  it('leaves no browser-reserved combo on a remapped command', () => {
    const left = remapped.flatMap(([id, combos]) =>
      combos.filter((c) => browserReserved.has(c)).map((c) => `${id} ${c}`),
    )
    expect(left).toEqual([])
  })

  it('clashes with no other command in an overlapping scope', () => {
    const clashes: string[] = []
    for (const [id, combos] of remapped) {
      const own = effective.find((c) => c.id === id)
      if (!own) continue
      for (const combo of combos) {
        for (const other of effective) {
          if (other.id !== id && other.shortcuts.includes(combo) && scopesOverlap(own.scope, other.scope)) {
            clashes.push(`${combo}: ${id} vs ${other.id}`)
          }
        }
      }
    }
    expect(clashes).toEqual([])
  })
})
