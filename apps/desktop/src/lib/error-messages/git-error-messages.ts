/**
 * Git-path friendly-error copy: `FriendlyGitErrorKind` → user-facing message.
 *
 * The git module classifies a git-shaped failure into a typed kind (verbatim
 * from the old Rust `git/friendly.rs`); this factory owns the words. Git copy is
 * git-domain and stays a distinct namespace from the listing reasons. All copy
 * here is fully static (no runtime-param interpolation), matching the old
 * `Markdown::literal` wrapping.
 *
 * The literal English lives in the `errors.git.*` message catalog and is pulled
 * via `getMessage()` (a RAW catalog lookup, never ICU `t()`): these strings carry
 * markdown and bypass ICU's brace/apostrophe grammar. See `$lib/intl`'s docs.
 */

import type { FriendlyErrorMessage } from './friendly-error-message'
import { getMessage } from '$lib/intl/messages.svelte'

/** Serialized camelCase from Rust `FriendlyGitErrorKind`. */
export type FriendlyGitErrorKind =
  | 'notARepo'
  | 'orphanedWorktree'
  | 'corruptRepo'
  | 'indexLocked'
  | 'permissionDenied'
  | 'bareRepo'
  | 'blobTooLarge'
  | 'shallowBoundary'
  | 'missingObject'
  | 'gitDirPermissionDenied'

/** Maps a git-error kind to its user-facing message (catalog copy). */
export function getGitErrorMessage(kind: FriendlyGitErrorKind): FriendlyErrorMessage {
  return {
    title: getMessage(`errors.git.${kind}.title`),
    message: getMessage(`errors.git.${kind}.message`),
    suggestion: getMessage(`errors.git.${kind}.suggestion`),
  }
}
