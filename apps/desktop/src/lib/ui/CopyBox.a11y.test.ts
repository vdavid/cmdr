/**
 * Tier 3 a11y tests for `CopyBox.svelte`.
 *
 * Checks that the monospace text + Copy button combo exposes a labelled button
 * and doesn't fall into any common axe traps.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CopyBox from './CopyBox.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

describe('CopyBox a11y', () => {
  it('default (short command) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CopyBox, { target, props: { text: 'ls -la' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long multi-argument command has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CopyBox, {
      target,
      props: { text: 'sudo defaults write com.apple.Finder AppleShowAllFiles -bool true' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a path with a shortened display and its own copy label has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CopyBox, {
      target,
      props: {
        text: '/Volumes/Naspolya/media/photos/2026/07-summer-archive/DSC09241.arw',
        displayText: '/Volumes/Naspolya/media/…/DSC09241.arw',
        copyAriaLabel: 'Copy path to clipboard',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
