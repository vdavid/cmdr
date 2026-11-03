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
import { getListingErrorMessage, type ListingErrorReason } from './listing-error-messages'
import { getGitErrorMessage } from './git-error-messages'
import { getProviderSuggestion } from './provider-error-messages'
import type { ListingError } from '#lib/ipc/bindings.js'

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

  // The key's permissions or a provider that paused the account (a usage cap, a
  // billing hold): one S3 answer for both, so the words name both.
  it('names both causes when an object store account refuses', () => {
    const message = getListingErrorMessage({ reason: 'objectStoreRefused', path: '/photos' })
    expect(message.title).not.toMatch(/^errors\./)
    expect(message.message).toContain('/photos')
    expect(message.suggestion).toContain('permissions')
    expect(message.suggestion).toContain('usage cap')
  })

  // A phone's refusal is Android's call: none of the macOS page's advice applies, and
  // the words carry no path, since a phone's app path carries its serial.
  it('words a folder a phone refuses without any macOS advice', () => {
    const message = getListingErrorMessage({ reason: 'phoneRefused' })
    expect(message.title).toBe('This phone keeps this folder private')
    expect(message.message).toContain('Android')
    expect(message.suggestion).toContain('shared storage')
    for (const text of [message.title, message.message, message.suggestion]) {
      expect(text).not.toMatch(/macOS|System Settings|Finder|Terminal|ls -la|\{/)
    }
  })

  // A connected server has no mount, so the mounted share's "eject it" advice can't help.
  it('words a folder a connected server refuses without any mount or macOS advice', () => {
    const message = getListingErrorMessage({ reason: 'serverRefused' })
    expect(message.title).toBe('The server won’t open this folder')
    expect(message.suggestion).toContain('account')
    for (const text of [message.title, message.message, message.suggestion]) {
      expect(text).not.toMatch(/macOS|System Settings|Finder|Terminal|[Ee]ject|mount|\{/)
    }
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

  // One reason asks for every password, and its typed `container` says what's
  // locked. The pane shows this behind the password prompt for as long as a repo
  // takes to check it, so the words must name the right thing, wherever the
  // path happens to be.
  describe('a locked container', () => {
    const lockedBy = (container: 'archive' | 'resticRepo'): ListingError => ({
      category: 'needs_action',
      reason: { reason: 'archiveNeedsPassword', container, wrongAttempt: false },
      provider: null,
      actionKind: null,
      retryHint: false,
      rawDetail: 'needs a password',
    })

    it('speaks of a backup when the restic repo is locked', () => {
      const rendered = renderListingError(lockedBy('resticRepo'))
      expect(rendered.title).toBe('This backup needs its password')
      expect(`${rendered.explanation} ${rendered.suggestion}`).not.toContain('archive')
      expect(rendered.listingReason).toBe('archiveNeedsPassword')
    })

    it('speaks of an archive when an archive is locked', () => {
      const locked = lockedBy('archive')
      const rendered = renderListingError(locked)
      expect(rendered.title).toBe(getListingErrorMessage(locked.reason as ListingErrorReason).title)
      expect(rendered.title).toContain('archive')
    })
  })
})
