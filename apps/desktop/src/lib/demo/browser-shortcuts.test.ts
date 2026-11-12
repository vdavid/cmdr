/**
 * The demo's browser-safe shortcut map (`browser-shortcuts.ts`) has to hold up as a real
 * customization on every visitor OS: every id a command, every combo one a keypress produces
 * there, no new clash, and none of the combos the browser takes before the page sees them. Off
 * macOS the arm is the registry's defaults minus the browser's, so it invents no key.
 */
import { beforeAll, describe, expect, it } from 'vitest'
import { COMMAND_IDS } from '#lib/commands/command-ids.js'
import { commands, defaultShortcuts } from '#lib/commands/command-registry.js'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import type { Platform } from '#lib/platform/index.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { scopesOverlap } from '#lib/shortcuts/scope-hierarchy.js'
import { BROWSER_WITHHELD, browserSafeShortcuts } from './browser-shortcuts.js'

/** The combos a browser keeps for itself on macOS: new tab or window, close tab, reopen tab, switch to tab N. */
const MAC_BROWSER_RESERVED = new Set([
  '⌘T',
  '⌘W',
  '⌘N',
  '⌘⇧T',
  ...Array.from({ length: 9 }, (_, i) => `⌘${String(i + 1)}`),
])

function withheldOn(platform: Platform): ReadonlySet<string> {
  return platform === 'macos' ? MAC_BROWSER_RESERVED : BROWSER_WITHHELD[platform]
}

type Flag = 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'

/** Each OS's modifier tokens as the registry spells them, with the flag each sets. */
const TOKENS: Record<Platform, readonly [token: string, flag: Flag][]> = {
  macos: [
    ['⌘', 'metaKey'],
    ['⌃', 'ctrlKey'],
    ['⌥', 'altKey'],
    ['⇧', 'shiftKey'],
  ],
  windows: [
    ['Ctrl+', 'ctrlKey'],
    ['Alt+', 'altKey'],
    ['Shift+', 'shiftKey'],
    ['Super+', 'metaKey'],
  ],
  linux: [
    ['Ctrl+', 'ctrlKey'],
    ['Alt+', 'altKey'],
    ['Shift+', 'shiftKey'],
    ['Super+', 'metaKey'],
  ],
}

const EVENT_KEYS: Record<string, string> = { '↑': 'ArrowUp', '↓': 'ArrowDown', '←': 'ArrowLeft', '→': 'ArrowRight' }

/** The keydown a stored combo stands for on `platform`. */
function keydownFor(combo: string, platform: Platform): KeyboardEvent {
  const event = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, key: '', code: '' }
  let rest = combo
  for (;;) {
    const match = TOKENS[platform].find(([token]) => rest.startsWith(token) && rest.length > token.length)
    if (match === undefined) break
    event[match[1]] = true
    rest = rest.slice(match[0].length)
  }
  event.key = EVENT_KEYS[rest] ?? (rest.length === 1 ? rest.toLowerCase() : rest)
  return event as unknown as KeyboardEvent
}

describe.each(PLATFORMS)('the browser-safe map for a visitor on %s', (platform) => {
  beforeAll(() => withPlatform(platform))

  const map = (): [string, string[]][] => Object.entries<string[]>(browserSafeShortcuts(platform))

  /** Every command's combos as the demo seeds them: the map's where it remaps, the registry's otherwise. */
  function effective(): { id: string; scope: string; shortcuts: readonly string[] }[] {
    const seeded = browserSafeShortcuts(platform) as Record<string, string[] | undefined>
    return commands.map((command) => ({
      id: command.id,
      scope: command.scope,
      shortcuts: seeded[command.id] ?? defaultShortcuts(command.id, platform),
    }))
  }

  it('remaps only real commands', () => {
    const known = new Set<string>(COMMAND_IDS)
    expect(
      map()
        .map(([id]) => id)
        .filter((id) => !known.has(id)),
    ).toEqual([])
  })

  it('spells every combo as a real keypress there produces it', () => {
    const misspelled = map().flatMap(([id, combos]) =>
      combos.filter((combo) => formatKeyCombo(keydownFor(combo, platform)) !== combo).map((c) => `${id}: ${c}`),
    )
    expect(misspelled).toEqual([])
  })

  it('leaves no combo the browser keeps on a remapped command', () => {
    const withheld = withheldOn(platform)
    const left = map().flatMap(([id, combos]) => combos.filter((c) => withheld.has(c)).map((c) => `${id}: ${c}`))
    expect(left).toEqual([])
  })

  it('clashes with no other command in an overlapping scope', () => {
    const all = effective()
    const clashes: string[] = []
    for (const [id, combos] of map()) {
      const own = all.find((c) => c.id === id)
      if (!own) continue
      for (const combo of combos) {
        for (const other of all) {
          if (other.id !== id && other.shortcuts.includes(combo) && scopesOverlap(own.scope, other.scope)) {
            clashes.push(`${combo}: ${id} vs ${other.id}`)
          }
        }
      }
    }
    expect(clashes).toEqual([])
  })
})

describe.each(['windows', 'linux'] as const)('the %s arm', (platform) => {
  beforeAll(() => withPlatform(platform))

  it('changes only commands holding a combo the browser keeps, and only by dropping it', () => {
    const withheld = BROWSER_WITHHELD[platform]
    for (const [id, combos] of Object.entries<string[]>(browserSafeShortcuts(platform))) {
      const defaults = defaultShortcuts(id, platform)
      expect(
        defaults.some((c) => withheld.has(c)),
        id,
      ).toBe(true)
      expect(combos, id).toEqual(defaults.filter((c) => !withheld.has(c)))
    }
  })

  it('leaves every command that holds no such combo to the registry', () => {
    const seeded = new Set(Object.keys(browserSafeShortcuts(platform)))
    const withheld = BROWSER_WITHHELD[platform]
    const missed = commands
      .filter((c) => defaultShortcuts(c.id, platform).some((combo) => withheld.has(combo)))
      .filter((c) => !seeded.has(c.id))
      .map((c) => c.id)
    expect(missed).toEqual([])
  })
})
