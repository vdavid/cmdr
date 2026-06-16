/**
 * Writing-rules enforcement for friendly-error copy, ported from the Rust
 * `error_messages_never_contain_error_or_failed` test (and the trivializing-word
 * checks). This is strictly better coverage than the old Rust test: it iterates
 * EVERY listing reason × representative params, EVERY provider × category, and
 * EVERY git kind, and checks the actual rendered output.
 *
 * Rules (see `docs/style-guide.md`): never "error" or "failed", no trivializing
 * words ("just", "simple", "easy").
 */

import { describe, expect, it } from 'vitest'
import { getListingErrorMessage, type ListingErrorReason } from './listing-error-messages'
import { getGitErrorMessage, type FriendlyGitErrorKind } from './git-error-messages'
import { getProviderSuggestion, type Provider, type ProviderCategory } from './provider-error-messages'
import type { FriendlyErrorMessage } from './friendly-error-message'

// "error" / "failed" are forbidden everywhere (the always-on rule). The
// trivializing words are also forbidden, EXCEPT where the pre-change Rust copy
// already used one: this is a behavior-preserving move (the spec forbids
// rewording), so a pre-existing nit is preserved verbatim and flagged here for a
// future copy pass rather than silently changed.
const NEVER_WORDS = ['error', 'failed']
const TRIVIALIZING_WORDS = ['just', 'simple', 'easy']

// Reasons whose pre-change copy already contains a trivializing word. The
// `tccRestricted` suggestion says "for just this folder". Flagged for a copy
// pass; NOT reworded here (behavior-preserving move).
const PREEXISTING_TRIVIALIZING_EXCEPTIONS = new Set<string>(['tccRestricted'])

const PATH = '/Volumes/x/folder/file.txt'

/** Word-boundary check: the token appears as a standalone word (Rust parity). */
function containsWord(text: string, word: string): boolean {
  return text
    .toLowerCase()
    .split(/\s+/)
    .some((w) => w.replace(/[^a-z]/g, '') === word)
}

function assertClean(label: string, m: FriendlyErrorMessage, checkTrivializing = true) {
  const words = checkTrivializing ? [...NEVER_WORDS, ...TRIVIALIZING_WORDS] : NEVER_WORDS
  for (const part of [m.title, m.message, m.suggestion]) {
    for (const word of words) {
      expect(containsWord(part, word), `${label}: copy contains forbidden word "${word}": ${part}`).toBe(false)
    }
  }
}

// Every listing reason with representative params.
const LISTING_REASONS: ListingErrorReason[] = [
  { reason: 'interrupted' },
  { reason: 'notEnoughMemory' },
  { reason: 'resourceBusy', path: PATH },
  { reason: 'temporarilyUnavailable' },
  { reason: 'networkDown' },
  { reason: 'networkConnectionDropped' },
  { reason: 'connectionDropped' },
  { reason: 'connectionReset' },
  { reason: 'connectionTimedOutErrno' },
  { reason: 'hostDown' },
  { reason: 'staleConnection' },
  { reason: 'lockUnavailable' },
  { reason: 'cancelledErrno' },
  { reason: 'notPermitted', path: PATH },
  { reason: 'pathNotFoundErrno', path: PATH },
  { reason: 'noPermissionErrno', path: PATH },
  { reason: 'alreadyExistsErrno', path: PATH },
  { reason: 'crossDeviceOperation' },
  { reason: 'notAFolder', path: PATH },
  { reason: 'isAFolderErrno', path: PATH },
  { reason: 'diskFullErrno' },
  { reason: 'readOnlyVolumeErrno' },
  { reason: 'notSupportedErrno' },
  { reason: 'networkUnreachable' },
  { reason: 'connectionRefused' },
  { reason: 'symlinkLoopErrno', path: PATH },
  { reason: 'nameTooLongErrno' },
  { reason: 'hostUnreachable' },
  { reason: 'folderNotEmpty', path: PATH },
  { reason: 'quotaExceeded' },
  { reason: 'authRequiredEauth' },
  { reason: 'authRequiredEneedauth' },
  { reason: 'devicePoweredOff' },
  { reason: 'attributeNotFound' },
  { reason: 'diskReadProblem', path: PATH },
  { reason: 'unexpectedSystemResponse' },
  { reason: 'deviceProblem' },
  { reason: 'couldntReadUnknown', path: PATH },
  { reason: 'notFound', path: PATH },
  { reason: 'tccRestricted', path: PATH },
  { reason: 'permissionDenied', path: PATH },
  { reason: 'alreadyExists', path: PATH },
  { reason: 'cancelled' },
  { reason: 'deviceDisconnected', path: PATH },
  { reason: 'readOnly' },
  { reason: 'storageFull' },
  { reason: 'connectionTimedOut' },
  { reason: 'notSupported' },
  { reason: 'deletePending', path: PATH },
  { reason: 'ioSerious', path: PATH, osMessage: 'something went wrong' },
  { reason: 'isADirectory', path: PATH },
  { reason: 'emptyRootICloud' },
]

const GIT_KINDS: FriendlyGitErrorKind[] = [
  'notARepo',
  'orphanedWorktree',
  'corruptRepo',
  'indexLocked',
  'permissionDenied',
  'bareRepo',
  'blobTooLarge',
  'shallowBoundary',
  'missingObject',
  'gitDirPermissionDenied',
]

const PROVIDERS: Provider[] = [
  'dropbox',
  'googleDrive',
  'oneDrive',
  'box',
  'pCloud',
  'nextcloud',
  'synologyDrive',
  'tresorit',
  'protonDrive',
  'sync',
  'egnyte',
  'macDroid',
  'iCloud',
  'pCloudFuse',
  'macFuse',
  'veraCrypt',
  'cmVolumes',
  'genericCloudStorage',
]

const CATEGORIES: ProviderCategory[] = ['transient', 'needs_action', 'serious']

describe('friendly-error copy obeys the writing rules', () => {
  for (const r of LISTING_REASONS) {
    it(`listing reason "${r.reason}" is clean`, () => {
      const checkTrivializing = !PREEXISTING_TRIVIALIZING_EXCEPTIONS.has(r.reason)
      assertClean(`listing:${r.reason}`, getListingErrorMessage(r), checkTrivializing)
    })
  }

  for (const kind of GIT_KINDS) {
    it(`git kind "${kind}" is clean`, () => {
      assertClean(`git:${kind}`, getGitErrorMessage(kind))
    })
  }

  for (const provider of PROVIDERS) {
    for (const cat of CATEGORIES) {
      it(`provider "${provider}" × "${cat}" suggestion is clean`, () => {
        const suggestion = getProviderSuggestion(provider, cat)
        for (const word of [...NEVER_WORDS, ...TRIVIALIZING_WORDS]) {
          expect(
            containsWord(suggestion, word),
            `provider ${provider}/${cat} contains "${word}": ${suggestion}`,
          ).toBe(false)
        }
      })
    }
  }
})
