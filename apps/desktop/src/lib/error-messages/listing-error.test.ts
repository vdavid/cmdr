/**
 * Tests for the typed `ListingError` → rendered `FriendlyError` adapter.
 *
 * Covers the three dispatch paths (listing reason, git reason, provider overlay)
 * and the field pass-through. The exact copy is asserted byte-for-byte by the
 * frozen golden in `friendly-error-parity.test.ts`; here we check the WIRING:
 * base selection, the provider-suggestion override, and that category / retry /
 * action / raw-detail ride through unchanged.
 */

import { describe, it, expect } from 'vitest'
import { renderListingError } from './listing-error'
import { getListingErrorMessage } from './listing-error-messages'
import { getGitErrorMessage } from './git-error-messages'
import { getProviderSuggestion } from './provider-error-messages'
import type { ListingError } from '$lib/ipc/bindings'

describe('renderListingError', () => {
  it('renders a plain listing reason from the listing factory', () => {
    const error: ListingError = {
      category: 'needs_action',
      reason: { reason: 'permissionDenied', path: '/some/_x_*y' },
      provider: null,
      actionKind: 'open_privacy_settings',
      retryHint: false,
      rawDetail: 'EACCES (os error 13)',
    }
    const base = getListingErrorMessage({ reason: 'permissionDenied', path: '/some/_x_*y' })

    const rendered = renderListingError(error)
    expect(rendered.title).toBe(base.title)
    expect(rendered.explanation).toBe(base.message)
    expect(rendered.suggestion).toBe(base.suggestion)
    expect(rendered.category).toBe('needs_action')
    expect(rendered.actionKind).toBe('open_privacy_settings')
    expect(rendered.retryHint).toBe(false)
    expect(rendered.rawDetail).toBe('EACCES (os error 13)')
  })

  it('renders the git reason from the git factory', () => {
    const error: ListingError = {
      category: 'needs_action',
      reason: { reason: 'git', kind: 'notARepo' },
      provider: null,
      actionKind: null,
      retryHint: false,
      rawDetail: 'git: NotARepo (path=/x)',
    }
    const base = getGitErrorMessage('notARepo')

    const rendered = renderListingError(error)
    expect(rendered.title).toBe(base.title)
    expect(rendered.explanation).toBe(base.message)
    expect(rendered.suggestion).toBe(base.suggestion)
  })

  it('overrides the suggestion with the provider-specific one when a provider is present', () => {
    const error: ListingError = {
      category: 'transient',
      reason: { reason: 'connectionTimedOut' },
      provider: 'dropbox',
      actionKind: null,
      retryHint: true,
      rawDetail: 'ETIMEDOUT',
    }
    const base = getListingErrorMessage({ reason: 'connectionTimedOut' })

    const rendered = renderListingError(error)
    // Title + explanation stay from the base reason; only the suggestion changes.
    expect(rendered.title).toBe(base.title)
    expect(rendered.explanation).toBe(base.message)
    expect(rendered.suggestion).toBe(getProviderSuggestion('dropbox', 'transient'))
    expect(rendered.suggestion).not.toBe(base.suggestion)
  })

  it('keeps the base suggestion when no provider was detected', () => {
    const error: ListingError = {
      category: 'transient',
      reason: { reason: 'connectionTimedOut' },
      provider: null,
      actionKind: null,
      retryHint: true,
      rawDetail: 'ETIMEDOUT',
    }
    const rendered = renderListingError(error)
    expect(rendered.suggestion).toBe(getListingErrorMessage({ reason: 'connectionTimedOut' }).suggestion)
  })
})
