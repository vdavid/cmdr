/**
 * Tier 3 a11y tests for `FriendlyErrorContent.svelte`.
 *
 * The component renders the backend-supplied `FriendlyError` (markdown
 * explanation + suggestion) inside the transfer error dialog. Markdown blocks
 * delegate anchor clicks to system-settings/external URL openers. We assert
 * the rendered DOM has no a11y violations across the three error categories
 * and across markdown shapes (bold, lists, links).
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FriendlyErrorContent from './FriendlyErrorContent.svelte'
import type { FriendlyError } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

function makeFriendly(overrides: Partial<FriendlyError> = {}): FriendlyError {
  return {
    category: 'serious',
    title: 'Whatever',
    explanation: 'Plain text explanation.',
    suggestion: 'Plain text suggestion.',
    rawDetail: 'detail',
    retryHint: false,
    ...overrides,
  }
}

describe('FriendlyErrorContent a11y', () => {
  it('serious category with plain text has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FriendlyErrorContent, { target, props: { friendly: makeFriendly() } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('transient category with bold + list has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FriendlyErrorContent, {
      target,
      props: {
        friendly: makeFriendly({
          category: 'transient',
          explanation: 'A **bold** thing happened.',
          suggestion: '- one\n- two\n- three',
          retryHint: true,
        }),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('needs_action category with markdown link has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FriendlyErrorContent, {
      target,
      props: {
        friendly: makeFriendly({
          category: 'needs_action',
          suggestion: '[Open settings](x-apple.systempreferences:com.apple.preference.security?Privacy)',
        }),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
