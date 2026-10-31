/**
 * Tier 3 a11y test for `SmbLinkConsentDialog.svelte`. The behaviour lives in
 * `SmbLinkConsentDialog.svelte.test.ts`; this pins that a reader hears the two
 * servers along with the question.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { computeAccessibleDescription } from 'dom-accessibility-api'
import SmbLinkConsentDialog from './SmbLinkConsentDialog.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  allowSmbLinkTarget: vi.fn(() => Promise.resolve(true)),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SmbLinkConsentDialog, {
    target,
    props: { server: 'nas.local', target: 'fs01.example.com', onAllowed: vi.fn(), onCancel: vi.fn() },
  })
  await tick()
  return target
}

describe('SmbLinkConsentDialog a11y', () => {
  it('has no a11y violations', async () => {
    const target = await mountDialog()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('is an alertdialog that describes itself with both servers', async () => {
    const target = await mountDialog()
    const dialog = target.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    const description = computeAccessibleDescription(dialog as Element)
    expect(description).toContain('nas.local')
    expect(description).toContain('fs01.example.com')
    target.remove()
  })
})
