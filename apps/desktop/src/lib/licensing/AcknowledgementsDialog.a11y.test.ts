/**
 * Tier 3 a11y tests for `AcknowledgementsDialog.svelte`.
 *
 * The dialog has two states worth checking: the brief loading state before the
 * generated package list resolves, and the loaded state with the two long
 * link lists. The lists are the interesting case, since hundreds of links in a
 * scrollable region is where a11y usually goes wrong.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AcknowledgementsDialog from './AcknowledgementsDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

// The real file is ~119 KB of generated JSON; a couple of representative rows
// exercise the same markup, including the URL-less case.
vi.mock('./third-party-packages.gen.json', () => ({
  default: {
    rust: [
      { name: 'serde', version: '1.0.228', license: 'MIT OR Apache-2.0', url: 'https://github.com/serde-rs/serde' },
      { name: 'mystery', version: '1.0.0', license: 'MIT', url: '' },
    ],
    npm: [{ name: '@ark-ui/svelte', version: '5.22.1', license: 'MIT', url: 'https://ark-ui.com' }],
  },
}))

function mountDialog(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AcknowledgementsDialog, { target, props: { onClose: () => {} } })
  return target
}

/**
 * Waits for the dynamic package-list import to land. A fixed number of `tick()`s
 * isn't enough (the `import()` settles over an unknown number of macrotasks), and
 * getting this wrong silently re-runs the loading-state assertions instead of the
 * loaded ones.
 */
async function waitForPackages(target: HTMLElement): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 5))
    await tick()
    if (target.querySelector('.package-list li')) return
  }
  throw new Error("The package list never rendered; the dialog's dynamic import didn't resolve")
}

describe('AcknowledgementsDialog a11y', () => {
  it('has no a11y violations while the list is loading', async () => {
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations once the package lists are rendered', async () => {
    const target = mountDialog()
    await waitForPackages(target)
    await expectNoA11yViolations(target)
  })
})
