/**
 * Tier 3 a11y tests for `AskCmdrWakeDigest.svelte`.
 *
 * The collapsed block that opens a thread the agent started for itself. `aria-expanded` on
 * the toggle is the load-bearing attribute: the whole breakdown is behind it, so a screen
 * reader has to be told there is something to open.
 */

import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrWakeDigest from './AskCmdrWakeDigest.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { WakeDigestFolderView, WakeDigestRollupView } from '$lib/tauri-commands'

function mountDigest(folders: WakeDigestFolderView[], rollups: WakeDigestRollupView[] = []): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrWakeDigest, { target, props: { folders, rollups } })
  return target
}

const downloads: WakeDigestFolderView = {
  folder: '/Users/dana/Downloads',
  created: 4,
  modified: 0,
  removed: 1,
  renamed: 0,
}

describe('AskCmdrWakeDigest a11y', () => {
  it('the collapsed block has no a11y violations', async () => {
    const target = mountDigest([downloads])
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the expanded block, rollup line included, has no a11y violations', async () => {
    const target = mountDigest([downloads], [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }])
    await tick()
    const toggle = target.querySelector<HTMLButtonElement>('.digest-toggle')
    if (toggle === null) throw new Error('expected a .digest-toggle button')
    toggle.click()
    await tick()
    expect(target.querySelector('.detail')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
