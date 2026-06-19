/**
 * Tier 3 a11y tests for `FirstConnectIndexToastContent.svelte`: the first-connect
 * "index this drive?" toast (heading, body, and three action buttons) must have
 * no axe violations.
 */
import { describe, it, expect } from 'vitest'
import { mount, flushSync } from 'svelte'
import FirstConnectIndexToastContent from './FirstConnectIndexToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function mountToast() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FirstConnectIndexToastContent, {
    target,
    props: {
      toastId: 'toast-1',
      volumeId: 'smb-backups',
      volumeName: 'Backups',
      onEnable: () => {},
      onSilenceDrive: () => {},
      onSilenceAll: () => {},
    },
  })
  flushSync()
  return target
}

describe('FirstConnectIndexToastContent a11y', () => {
  it('the rendered toast has no violations', async () => {
    const target = mountToast()
    expect(target.querySelector('.first-connect-toast')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
