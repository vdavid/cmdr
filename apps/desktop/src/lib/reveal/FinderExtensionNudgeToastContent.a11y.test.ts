import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('./finder-extension-nudge-answer', () => ({
  declineFinderExtensionNudge: vi.fn(),
  acceptFinderExtensionNudge: vi.fn(() => Promise.resolve()),
}))

import FinderExtensionNudgeToastContent from './FinderExtensionNudgeToastContent.svelte'

describe('the Finder extension offer toast', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FinderExtensionNudgeToastContent, { target, props: { toastId: 'finder-extension-nudge' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
