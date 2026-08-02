/**
 * Adapter: typed `ListingError` (from the wire) → rendered friendly copy.
 *
 * The Rust backend ships a typed, word-free `ListingError` (category, reason +
 * params, optional provider, action kind, retry hint, raw detail). This adapter
 * owns turning that into the displayable shape `ErrorPane` renders: it picks the
 * base message from the listing factory (or the git factory for the `git`
 * reason), then applies the provider-suggestion override when a provider is
 * present — reproducing the old Rust `enrich_with_provider` override exactly.
 *
 * Markdown escaping of runtime params already happens inside the factories
 * (`esc(...)`); the composed `explanation` / `suggestion` are trusted markdown
 * rendered through the single `renderErrorMarkdown` → `snarkdown` site.
 */

import type { ListingError, ListingErrorReason, ErrorCategory } from '$lib/ipc/bindings'
import type { FriendlyError } from '$lib/file-explorer/types'
import { getListingErrorMessage } from './listing-error-messages'
import { getGitErrorMessage } from './git-error-messages'
import { getProviderSuggestion, type ProviderCategory } from './provider-error-messages'

/** Maps the wire `ErrorCategory` to the provider table's category key. */
function providerCategory(category: ErrorCategory): ProviderCategory {
  // `ErrorCategory` and `ProviderCategory` share the same three values.
  return category
}

/**
 * Turns a typed `ListingError` into the rendered `FriendlyError` shape
 * `ErrorPane` consumes (title + trusted-markdown explanation/suggestion +
 * category + raw detail + retry/action affordances).
 */
export function renderListingError(error: ListingError): FriendlyError {
  const base = baseMessage(error.reason)

  // Provider overlay: when the backend detected a provider, the provider-
  // specific suggestion replaces the base reason's suggestion (mirrors the old
  // Rust `enrich_with_provider`). The git reason carries no provider.
  const suggestion =
    error.provider != null ? getProviderSuggestion(error.provider, providerCategory(error.category)) : base.suggestion

  return {
    category: error.category,
    title: base.title,
    explanation: base.message,
    suggestion,
    rawDetail: error.rawDetail,
    retryHint: error.retryHint,
    actionKind: error.actionKind,
  }
}

/** Picks the base message: the git factory for `git`, else the listing factory. */
function baseMessage(reason: ListingErrorReason) {
  if (reason.reason === 'git') {
    return getGitErrorMessage(reason.kind)
  }
  // Every non-git wire reason is a member of the listing factory's union.
  return getListingErrorMessage(reason)
}
