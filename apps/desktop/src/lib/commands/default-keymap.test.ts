/**
 * The default keymap a user gets on each OS, pinned as one reviewed file per platform in
 * `__snapshots__/default-keymap.{macos,windows,linux}.txt`. Any change to a default key shows up
 * as a line diff there, per platform, so "macOS didn't move" and "exactly these keys moved" are
 * provable from the diff.
 *
 * Each line is `<command id>\t<combo>, <combo>` in registry order. A command with no key is
 * listed with an empty list; a command the OS can't offer (`needsCapability` off under that OS's
 * `withCapabilitiesPossibleOn` fixture) is left out. It reads through the public
 * `getDefaultShortcuts`, so the registry's internals can change under it.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import type { Platform } from '#lib/platform/index.js'
import { withCapabilitiesPossibleOn, withPlatform } from '#lib/platform/test-platform.js'
import { commandOffered, commands } from '#lib/commands/command-registry.js'
import { getDefaultShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { PLATFORMS } from './platform-shortcuts'

describe.each(PLATFORMS satisfies readonly Platform[])('default keymap on %s', (platform) => {
  const undos: (() => void)[] = []

  beforeAll(() => {
    undos.push(withPlatform(platform), withCapabilitiesPossibleOn(platform))
  })

  afterAll(() => {
    for (const undo of undos.reverse()) undo()
  })

  it('matches the pinned keymap', async () => {
    const lines = commands
      .filter(commandOffered)
      .map((command) => `${command.id}\t${getDefaultShortcuts(command.id).join(', ')}`)
    await expect(`${lines.join('\n')}\n`).toMatchFileSnapshot(`./__snapshots__/default-keymap.${platform}.txt`)
  })
})
