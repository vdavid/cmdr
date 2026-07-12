/**
 * Tier 3 a11y tests for `AskCmdrAttachmentChip.svelte`: a file/folder reference chip,
 * read-only under a sent message and removable in the composer. The remove button carries
 * an accessible label.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrAttachmentChip from './AskCmdrAttachmentChip.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { AttachmentRef } from '$lib/tauri-commands'

const fileRef: AttachmentRef = { path: '/Users/me/taxes.pdf', kind: 'file' }
const folderRef: AttachmentRef = { path: '/Users/me/photos', kind: 'folder' }

function mountChip(attachment: AttachmentRef, onRemove?: (path: string) => void): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrAttachmentChip, { target, props: { attachment, onRemove } })
  return target
}

describe('AskCmdrAttachmentChip a11y', () => {
  it('a read-only file chip has no a11y violations', async () => {
    const target = mountChip(fileRef)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a removable folder chip has no a11y violations', async () => {
    const target = mountChip(folderRef, () => {})
    await tick()
    expect(target.querySelector('.chip-remove')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
