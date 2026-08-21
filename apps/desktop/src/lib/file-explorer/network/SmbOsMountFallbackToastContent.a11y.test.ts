import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./direct-connect', () => ({
  connectDirectly: vi.fn(() => Promise.resolve('connected')),
}))

vi.mock('./smb-login-hosts', () => ({
  promptForSmbCredentials: vi.fn(() => true),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

describe('SmbOsMountFallbackToastContent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbOsMountFallbackToastContent, {
      target,
      props: { toastId: 'smb-os-mount:smb-archive', volumeId: 'smb-archive', share: 'archive' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
