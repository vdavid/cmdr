/**
 * Tier 3 a11y tests for `RevealHandlerCard.svelte`.
 *
 * ❗ It can't join `sections.a11y.test.ts`. The card renders only on macOS, and
 * jsdom's user agent says otherwise, so auditing it there would audit an empty
 * container and call it covered. Forcing `isMacOS()` true file-wide would also
 * change what `KeyboardShortcutsSection` renders in the same run, so the mock
 * has to stay local — the same reason `AskCmdrSection` and the image-index
 * components keep their own files.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RevealHandlerState, RevealHandlerStatus } from '$lib/ipc/bindings'

const getRevealHandlerState = vi.fn<() => Promise<RevealHandlerStatus>>()

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getRevealHandlerState: () => getRevealHandlerState(),
    setRevealHandlerEnabled: () => Promise.resolve(unblocked({ kind: 'registered' })),
  },
}))

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/shortcuts/key-capture')>()),
  isMacOS: () => true,
}))

import RevealHandlerCard from './RevealHandlerCard.svelte'

/** A status with nothing standing in the way of the switch. */
function unblocked(state: RevealHandlerState): RevealHandlerStatus {
  return { state, blockedBy: null }
}

function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

async function mountCard(): Promise<HTMLDivElement> {
  const target = container()
  mount(RevealHandlerCard, { target, props: { searchQuery: '' } })
  await tick()
  await tick()
  return target
}

describe('RevealHandlerCard a11y', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('has no a11y violations when nobody holds the key', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'notRegistered' }))

    const target = await mountCard()

    // Guard against auditing an empty container: the whole risk of a
    // conditionally-rendered row is a green audit of nothing.
    expect(target.querySelector('.reveal-row')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while another app holds the key', async () => {
    getRevealHandlerState.mockResolvedValue(
      unblocked({ kind: 'heldByOtherApp', bundleId: 'com.cocoatech.PathFinder', displayName: 'Path Finder' }),
    )

    const target = await mountCard()

    expect(target.querySelector('.reveal-holder')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while the uninstall warning is up', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard()

    expect(target.querySelector('[data-test="reveal-handler-warning"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while the switch is blocked', async () => {
    // The reason is a tooltip, which no screen reader reads, so it's in the row as
    // `sr-only` text too. A disabled switch with no explanation anywhere would leave a
    // screen-reader user with a dead control and no idea why.
    getRevealHandlerState.mockResolvedValue({ state: { kind: 'notRegistered' }, blockedBy: 'notInApplications' })

    const target = await mountCard()

    expect(target.querySelector('[data-test="reveal-handler-blocked"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
