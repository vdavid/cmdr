/**
 * Rules the Windows and Linux default keymaps keep, checked over every command's RAW arm for that
 * platform (capability ignored), so a gated command's keys obey them before its capability flips on.
 * The decided keys live in `docs/notes/windows-port/keymap-decisions.md`; canonical spelling and
 * modifier order are `shortcut-vocabulary.test.ts`'s job.
 */

import { describe, it, expect } from 'vitest'
import { commands } from '#lib/commands/command-registry.js'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import { systemReservation } from '#lib/settings/sections/system-reserved-keys.js'
import { scopesOverlap } from './scope-hierarchy'

const OFF_MAC_PLATFORMS = ['windows', 'linux'] as const

function rawDefaults(platform: 'windows' | 'linux'): string[] {
  return commands.flatMap((command) => command.shortcuts[platform].map((combo) => `${command.id}: ${combo}`))
}

/** A key the OS (Windows) or some common desktop (Linux) takes before Cmdr sees it: Settings' reserved table. */
const takenOn = (platform: 'windows' | 'linux', combo: string): boolean => systemReservation(combo, platform) !== null

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

describe('default keys against what each OS takes first', () => {
  it('never put a default on a key Windows itself takes', () => {
    expect(rawDefaults('windows').filter((line) => takenOn('windows', line.split(': ')[1]))).toEqual([])
  })

  // A desktop that takes the key leaves the command keyless there, so the other key carries it.
  const holdingADesktopKey = commands.filter((command) => command.shortcuts.linux.some((c) => takenOn('linux', c)))

  it('find the Linux defaults some desktop takes (the check below has something to bite)', () => {
    expect(holdingADesktopKey.map((command) => command.id)).toContain('pane.rightVolumeChooser')
  })

  it.each(holdingADesktopKey.map((command) => [command.id, command.shortcuts.linux] as const))(
    '%s also holds a Linux key no listed desktop takes',
    (_id, keys) => {
      expect(keys.filter((combo) => !takenOn('linux', combo))).not.toEqual([])
    },
  )
})
