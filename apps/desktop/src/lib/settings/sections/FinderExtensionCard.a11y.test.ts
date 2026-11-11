/**
 * Tier 3 a11y tests for `FinderExtensionCard.svelte`. Its own file, because macOS file-wide
 * would change what other sections render: same reason as `RevealHandlerCard.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { FinderExtensionStatus } from '#lib/ipc/bindings.js'

const getFinderExtensionStatus = vi.fn<() => Promise<FinderExtensionStatus>>()

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    getFinderExtensionStatus: () => getFinderExtensionStatus(),
    setFinderExtensionEnabled: () => Promise.resolve({ enabled: true, blockedBy: null }),
  },
}))

vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))

import FinderExtensionCard from './FinderExtensionCard.svelte'

async function mountCard(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FinderExtensionCard, { target, props: { searchQuery: '' } })
  await tick()
  await tick()
  return target
}

describe('FinderExtensionCard a11y', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('has no a11y violations while the switch is free', async () => {
    getFinderExtensionStatus.mockResolvedValue({ enabled: false, blockedBy: null })
    const target = await mountCard()
    // Guard against auditing an empty container.
    expect(target.querySelector('.row')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while the switch is blocked', async () => {
    getFinderExtensionStatus.mockResolvedValue({ enabled: false, blockedBy: 'notProductionBuild' })
    const target = await mountCard()
    expect(target.querySelector('[data-test="finder-extension-blocked"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
