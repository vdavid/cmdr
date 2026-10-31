/**
 * Behavior tests for the error screen's action row.
 *
 * The row is the only way out of a failed listing, so which buttons appear is
 * load-bearing: `retryHint` is the backend's "retrying might help" signal (set on
 * Serious and NeedsAction errors too, not just Transient), and `canGoBack` must
 * hide a Go back that would silently no-op.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ErrorPane from './ErrorPane.svelte'
import type { FriendlyError } from '../types'

vi.mock('#lib/tauri-commands/index.js', () => ({
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/shortcuts/key-capture.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/shortcuts/key-capture.js')>()),
  isMacOS: vi.fn(() => true),
}))

const openErrorReportFor = vi.hoisted(() => vi.fn())
vi.mock('#lib/error-reporter/error-report-flow.svelte.js', () => ({ openErrorReportFor }))

const base = {
  title: 'Something went wrong',
  explanation: 'The folder did not open.',
  suggestion: 'Try one of these.',
  rawDetail: 'EIO',
} satisfies Omit<FriendlyError, 'category' | 'retryHint'>

describe('a folder behind a link to another server', () => {
  const friendly: FriendlyError = {
    ...base,
    category: 'needs_action',
    retryHint: false,
    linkConsent: { server: 'nas', target: 'fs01' },
  }

  it('offers the review, which asks through its callback', async () => {
    const onReviewLinkConsent = vi.fn()
    const target = mountPane({ friendly, onReviewLinkConsent })
    await tick()
    const review = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Review sign-in'))
    review?.click()
    expect(onReviewLinkConsent).toHaveBeenCalledOnce()
    target.remove()
  })

  it('offers no review on any other error', async () => {
    const target = mountPane({ friendly: { ...base, category: 'transient', retryHint: true } })
    await tick()
    expect(target.textContent).not.toContain('Review sign-in')
    target.remove()
  })
})

function mountPane(props: Partial<Parameters<typeof ErrorPane>[1]> & { friendly: FriendlyError }): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ErrorPane, { target, props: { folderPath: '/Volumes/x/y', ...props } as never })
  return target
}

function buttonTexts(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
}

describe('ErrorPane action row', () => {
  it('offers Try again whenever the backend set retryHint, not only on transient errors', async () => {
    // `retry_hint: true` is set on Serious errnos too (the `serious` helper in
    // `friendly_error/errno.rs` documents "retry hint on"), and on the NeedsAction
    // `emptyRootICloud`, whose doc comment promises a Try again button so the user
    // can re-list after granting access. Gating on `category === 'transient'` as
    // well silently swallowed all six.
    for (const category of ['transient', 'serious', 'needs_action'] as const) {
      const target = mountPane({ friendly: { ...base, category, retryHint: true } })
      await tick()
      expect(buttonTexts(target), `category=${category}`).toContain('Try again')
    }
  })

  it('hides Try again when the backend did not set retryHint', async () => {
    const target = mountPane({ friendly: { ...base, category: 'serious', retryHint: false } })
    await tick()
    expect(buttonTexts(target)).not.toContain('Try again')
  })

  it('always offers Go to home folder, even for an error with no CTA of its own', async () => {
    const target = mountPane({ friendly: { ...base, category: 'serious', retryHint: false } })
    await tick()
    expect(buttonTexts(target).some((t) => t.startsWith('Go to home folder'))).toBe(true)
  })

  it('offers Go back only when the tab has somewhere to go back to', async () => {
    const friendly: FriendlyError = { ...base, category: 'serious', retryHint: false }

    const without = mountPane({ friendly, canGoBack: false })
    await tick()
    expect(without.textContent).not.toContain('Go back')

    const with_ = mountPane({ friendly, canGoBack: true })
    await tick()
    expect(with_.textContent).toContain('Go back')
  })
})

describe('ErrorPane Send error report…', () => {
  // The other side said no on purpose, so there's no bug to report.
  it.each(['phoneRefused', 'serverRefused', 'remotePermissionDenied', 'objectStoreRefused'] as const)(
    'offers no report when %s',
    async (listingReason) => {
      const target = mountPane({ friendly: { ...base, category: 'needs_action', retryHint: false, listingReason } })
      await tick()
      expect(buttonTexts(target)).not.toContain('Send error report…')
    },
  )

  it('still offers a report for a local permission refusal, and for an untyped failure', async () => {
    const local = mountPane({
      friendly: { ...base, category: 'needs_action', retryHint: false, listingReason: 'permissionDenied' },
    })
    const untyped = mountPane({ friendly: { ...base, category: 'serious', retryHint: false } })
    await tick()
    expect(buttonTexts(local)).toContain('Send error report…')
    expect(buttonTexts(untyped)).toContain('Send error report…')
  })

  it('offers a report naming the typed listing reason and the title it shows', async () => {
    const target = mountPane({
      friendly: { ...base, category: 'transient', retryHint: true, listingReason: 'networkDown' },
    })
    await tick()
    Array.from(target.querySelectorAll('button'))
      .find((b) => b.textContent.trim() === 'Send error report…')
      ?.click()
    expect(openErrorReportFor).toHaveBeenCalledWith({
      kind: 'listingFailed',
      reason: 'networkDown',
      title: 'Something went wrong',
    })
  })
})
