/**
 * Rules the Windows and Linux default keymaps keep, checked over every command's RAW arm for that
 * platform (capability ignored), so a gated command's keys obey them before its capability flips on.
 * The decided keys live in `docs/notes/windows-port/keymap-decisions.md`; canonical spelling and
 * modifier order are `shortcut-vocabulary.test.ts`'s job.
 */

import { describe, it, expect } from 'vitest'
import { commands } from '#lib/commands/command-registry.js'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import { scopesOverlap } from './scope-hierarchy'

const OFF_MAC_PLATFORMS = ['windows', 'linux'] as const

function rawDefaults(platform: 'windows' | 'linux'): string[] {
  return commands.flatMap((command) => command.shortcuts[platform].map((combo) => `${command.id}: ${combo}`))
}

/**
 * The Linux keys some common desktop (GNOME, KDE Plasma, Xfce, Cinnamon, Ubuntu) takes before an
 * app sees them, among the keys Cmdr binds: `keymap-decisions.md` § "Reservations behind the Linux
 * keys", "Dead somewhere". Named by hand, until Settings' reserved-key table covers Linux.
 */
const TAKEN_BY_A_LINUX_DESKTOP: ReadonlySet<string> = new Set([
  'Alt+F1', // KDE, Xfce, Cinnamon
  'Alt+F2', // all five
  'Alt+F5', // GNOME, Cinnamon
  'Alt+F7', // GNOME, Xfce, Cinnamon
  'Ctrl+F1', // KDE, Xfce
  'Ctrl+F2', // KDE, Xfce
  'Ctrl+F3', // KDE, Xfce
  'Ctrl+F4', // KDE, Xfce
  'Ctrl+F5', // Xfce
  'Ctrl+F6', // Xfce
  'Ctrl+Alt+←', // GNOME, Xfce, Cinnamon
  'Ctrl+Alt+→', // GNOME, Xfce, Cinnamon
])

/** The commands whose FIRST Linux key a desktop takes, so each needs a second key. */
const FIRST_LINUX_KEY_TAKEN = ['file.compress', 'pane.leftVolumeChooser', 'pane.rightVolumeChooser']

describe.each(OFF_MAC_PLATFORMS)('default keys on %s', (platform) => {
  it('hold no Ctrl+Alt+<letter>, which AltGr layouts type characters with', () => {
    // AltGr arrives as Ctrl+Alt on Windows, so these collide with typing on Hungarian, German,
    // Polish, Czech, and more. Ctrl+Alt with an arrow or an F-key types nothing, so it's fine.
    const ctrlAltLetter = /^Ctrl\+Alt\+(Shift\+)?[A-Z]$/
    expect(rawDefaults(platform).filter((line) => ctrlAltLetter.test(line.split(': ')[1]))).toEqual([])
  })

  it('hold no Super key, which the desktop and Windows itself own', () => {
    expect(rawDefaults(platform).filter((line) => line.split(': ')[1].includes('Super+'))).toEqual([])
  })
})

/**
 * `registry-conflicts.test.ts` checks what a user on each OS gets, so a command that OS can't offer
 * yet holds no key there and can't clash. Its raw keys still can, unseen until its capability flips
 * on: this checks them all, every command on every OS, with no capability taken into account.
 */
describe.each(PLATFORMS)('raw default keys on %s', (platform) => {
  it('never put one combo on two commands in overlapping scopes', () => {
    const clashes: string[] = []
    commands.forEach((a, i) => {
      for (const b of commands.slice(i + 1)) {
        if (!scopesOverlap(a.scope, b.scope)) continue
        for (const combo of a.shortcuts[platform]) {
          if (b.shortcuts[platform].includes(combo)) clashes.push(`${combo}: ${a.id} vs ${b.id}`)
        }
      }
    })
    expect(clashes).toEqual([])
  })
})

describe('default keys on linux, against the desktops', () => {
  const linuxKeys = (id: string): readonly string[] =>
    commands.find((command) => command.id === id)?.shortcuts.linux ?? []

  it('name every command whose first key a desktop takes', () => {
    const taken = commands
      .filter((command) => TAKEN_BY_A_LINUX_DESKTOP.has(command.shortcuts.linux[0] ?? ''))
      .map((command) => command.id)
      .sort()
    expect(taken).toEqual(FIRST_LINUX_KEY_TAKEN)
  })

  it.each(FIRST_LINUX_KEY_TAKEN)('%s has a second key no listed desktop takes', (id) => {
    const free = linuxKeys(id)
      .slice(1)
      .filter((combo) => !TAKEN_BY_A_LINUX_DESKTOP.has(combo))
    expect(free.length).toBeGreaterThan(0)
  })
})
