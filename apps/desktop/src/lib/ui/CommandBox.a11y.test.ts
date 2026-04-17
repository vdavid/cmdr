/**
 * Tier 3 a11y tests for `CommandBox.svelte`.
 *
 * Checks that the monospace command + Copy button combo exposes a labelled
 * button and doesn't fall into any common axe traps.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CommandBox from './CommandBox.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

describe('CommandBox a11y', () => {
  it('default (short command) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CommandBox, { target, props: { command: 'ls -la' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long multi-argument command has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CommandBox, {
      target,
      props: { command: 'sudo defaults write com.apple.Finder AppleShowAllFiles -bool true' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
