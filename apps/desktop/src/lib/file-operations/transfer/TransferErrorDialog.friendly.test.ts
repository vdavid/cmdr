/**
 * Tests for `TransferErrorDialog.svelte`'s backend-`FriendlyError` rendering path.
 *
 * The component has two sources of copy:
 *   - `friendlyError` prop (backend `WriteErrorEvent.friendly` payload): preferred
 *   - `getUserFriendlyMessage(error, operationType)`: fallback for events without friendly
 *
 * The companion `TransferErrorDialog.a11y.test.ts` exercises the fallback path.
 * This file pins the friendly-path behaviour: title comes from `friendlyError.title`,
 * markdown is rendered (explanation and suggestion), category drives icon and
 * container colour, retry button visibility tracks `retryHint` / category.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferErrorDialog from './TransferErrorDialog.svelte'
import type { FriendlyError } from '$lib/file-explorer/types'
import type { Markdown } from '$lib/ipc/bindings'

// The `Markdown` brand is enforced by the wire shape; in tests we forge it
// to author fixtures with plain string literals.
const md = (s: string): Markdown => s as Markdown

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

function makeFriendly(overrides: Partial<FriendlyError> = {}): FriendlyError {
  return {
    category: 'serious',
    title: 'Backend Title',
    explanation: md('Backend explanation with **bold** text.'),
    suggestion: md('- step one\n- step two'),
    rawDetail: 'STATUS_TEST (os error 42)',
    retryHint: false,
    ...overrides,
  }
}

function mountDialog(props: {
  friendlyError?: FriendlyError
  onRetry?: () => void
  operationType?: 'copy' | 'move' | 'delete' | 'trash'
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferErrorDialog, {
    target,
    props: {
      operationType: props.operationType ?? 'move',
      error: { type: 'io_error', path: '/test/path', message: 'whatever' },
      friendlyError: props.friendlyError,
      onClose: () => {},
      ...(props.onRetry ? { onRetry: props.onRetry } : {}),
    },
  })
  return target
}

describe('TransferErrorDialog: friendlyError prop', () => {
  it('renders friendlyError.title instead of the fallback variant title', async () => {
    const target = mountDialog({ friendlyError: makeFriendly({ title: 'Backend says hello' }) })
    await tick()

    expect(target.textContent).toContain('Backend says hello')
    // Fallback would have surfaced "Move failed" for io_error; assert it didn't.
    expect(target.textContent).not.toContain('Move failed')
  })

  it('renders markdown in explanation and suggestion (bold and list)', async () => {
    const target = mountDialog({
      friendlyError: makeFriendly({
        explanation: md("Cmdr couldn't finish. Try **opening Finder** first."),
        suggestion: md('- one\n- two\n- three'),
      }),
    })
    await tick()

    // Bold rendered as <strong>
    const strong = target.querySelector('strong')
    expect(strong?.textContent).toBe('opening Finder')

    // Suggestion list items rendered as <li>
    const items = target.querySelectorAll('li')
    expect(items.length).toBe(3)
    expect(items[0].textContent).toBe('one')
  })

  it('uses error styling for serious category (red bg + CircleAlert icon)', async () => {
    const target = mountDialog({ friendlyError: makeFriendly({ category: 'serious' }) })
    await tick()

    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-error')
  })

  it('uses warning styling for transient category (TriangleAlert icon)', async () => {
    const target = mountDialog({ friendlyError: makeFriendly({ category: 'transient' }) })
    await tick()

    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-warning')
  })

  it('uses neutral styling for needs_action category (Info icon)', async () => {
    const target = mountDialog({ friendlyError: makeFriendly({ category: 'needs_action' }) })
    await tick()

    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-info')
  })

  it('renders Retry button when category is transient (even without retryHint)', async () => {
    const target = mountDialog({
      friendlyError: makeFriendly({ category: 'transient', retryHint: false }),
      onRetry: () => {},
    })
    await tick()

    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).toContain('Retry')
  })

  it('renders Retry button when retryHint is true (any category)', async () => {
    const target = mountDialog({
      friendlyError: makeFriendly({ category: 'serious', retryHint: true }),
      onRetry: () => {},
    })
    await tick()

    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).toContain('Retry')
  })

  it('hides Retry when category is needs_action and retryHint is false (even if onRetry is wired)', async () => {
    const target = mountDialog({
      friendlyError: makeFriendly({ category: 'needs_action', retryHint: false }),
      onRetry: () => {},
    })
    await tick()

    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).not.toContain('Retry')
  })

  it('uses friendlyError.rawDetail for the technical-details textarea', async () => {
    const target = mountDialog({
      friendlyError: makeFriendly({ rawDetail: 'STATUS_OBJECT_NAME_COLLISION (os error 17)' }),
    })
    await tick()

    // Open the disclosure
    const toggle = target.querySelector<HTMLButtonElement>('.details-toggle')
    toggle?.click()
    await tick()

    const textarea = target.querySelector<HTMLTextAreaElement>('.details-text')
    expect(textarea?.value).toBe('STATUS_OBJECT_NAME_COLLISION (os error 17)')
  })

  it('falls back to FE-derived copy when no friendlyError prop is supplied', async () => {
    const target = mountDialog({}) // no friendlyError
    await tick()

    // io_error variant → fallback title is "Move failed" (operationType: 'move')
    expect(target.textContent).toContain('Move failed')
  })
})
