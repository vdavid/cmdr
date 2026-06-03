import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'

describe('LatestDownloadEmptyToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LatestDownloadEmptyToastContent, {
      target,
      props: { toastId: 'test-empty-toast', onGoToDownloads: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
