/**
 * Writing-rules enforcement for friendly-error copy, ported from the Rust
 * `error_messages_never_contain_error_or_failed` test (and the trivializing-word
 * checks). This is strictly better coverage than the old Rust test: it iterates
 * EVERY listing reason × representative params, EVERY provider × category, and
 * EVERY git kind, and checks the actual rendered output.
 *
 * Rules (see `docs/style-guide.md`): never "error" or "failed", no trivializing
 * words ("just", "simple", "easy").
 *
 * ❗ The servers copy is here for the same reason: `servers.refusal.*` and
 * `servers.paneState.*` are what a person reads when a connect stopped, which
 * makes them error copy however they're filed, and they reach the screen through
 * a `Record` rather than the friendly-error pipeline this file was built for.
 * `adb.connect.*` joins them: same surface (`RemoteConnectView`), same rules, and
 * one more of its own — ❌ never a serial, a diagnostic, or a protocol word.
 */

import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'
import { wordConnectRefusal, type ConnectRefusalKind } from '$lib/servers/connect-refusals'
import { readAdbConnectOutcome } from '$lib/adb/adb-connect-errors'
import type { AdbConnectOutcomeError } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'
import serversCatalog from '$lib/intl/messages/en/servers.json'
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
  { reason: 'remotePermissionDenied', path: PATH },
  { reason: 'alreadyExists', path: PATH },
  { reason: 'cancelled' },
  { reason: 'deviceDisconnected', path: PATH },
  { reason: 'deviceReconnecting', path: PATH },
  { reason: 'notConnected', path: PATH },
  { reason: 'readOnly' },
  { reason: 'storageFull' },
  { reason: 'connectionTimedOut' },
  { reason: 'notSupported' },
  { reason: 'deletePending', path: PATH },
  { reason: 'invalidName', path: PATH },
  { reason: 'ioSerious', path: PATH, osMessage: 'something went wrong' },
  { reason: 'isADirectory', path: PATH },
  { reason: 'archiveUnreadable' },
  { reason: 'archiveNeedsPassword', wrongAttempt: false },
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
          expect(containsWord(suggestion, word), `provider ${provider}/${cat} contains "${word}": ${suggestion}`).toBe(
            false,
          )
        }
      })
    }
  }
})

// ── The servers copy ─────────────────────────────────────────────────────────

/**
 * Every refusal reason. Adding one makes this fail to typecheck, which is what
 * keeps a new reason from reaching a person unread.
 */
const REFUSAL_KINDS: ConnectRefusalKind[] = [
  'authentication_rejected',
  'needs_credentials',
  'auth_method_unsupported',
  'certificate_untrusted',
  'not_a_webdav_server',
  'invalid_url',
  'timed_out',
  'unreachable',
  'host_key_untrusted',
  'host_key_revoked',
]

/**
 * The pane's own sentences, which say what is happening rather than why it
 * stopped.
 *
 * ❗ **Derived from the catalog by PREFIX, ❌ never hand-spelled.** A written-out
 * list only covers what somebody remembered to add to it, and this one had
 * silently fallen ten keys behind the catalog: a new pane sentence with "failed"
 * in it shipped unguarded. Everything under `servers.paneState.` is copy a person
 * reads when a connect stopped, so everything under it is in scope by
 * construction.
 */
const PANE_STATE_KEYS = Object.keys(serversCatalog).filter(
  (key) => key.startsWith('servers.paneState.') && !key.startsWith('@'),
) as MessageKey[]

/**
 * Every placeholder any of those keys takes, so one loop can render them all.
 *
 * A key that doesn't take one ignores the extra, and the check is about the
 * WORDS around the placeholder rather than the value in it.
 */
const PANE_STATE_PARAMS = {
  name: 'Naspolya',
  duration: '2 minutes',
  // The two duration keys carry an ICU plural: the raw number selects the form,
  // the `*Text` twin is what the reader sees (`$lib/intl/messages/CLAUDE.md`).
  seconds: 60,
  secondsText: '60',
  minutes: 2,
  minutesText: '2',
}

/** The serial a refusal carries, which must never reach the sentence. */
const ADB_SERIAL = 'R58M12345'

/**
 * Every variant the backend can answer. The list is spelled out rather than
 * derived, so a new arm is a compile error here as well as in the `Record` the
 * reader is built on.
 */
const ADB_CONNECT_ERRORS: AdbConnectOutcomeError[] = [
  { type: 'adbNotInstalled' },
  { type: 'serverUnreachable' },
  { type: 'deviceGone', serial: ADB_SERIAL },
  { type: 'unauthorized', serial: ADB_SERIAL },
  { type: 'deviceTooOld', serial: ADB_SERIAL },
  { type: 'timedOut' },
  { type: 'cancelled' },
  { type: 'transport' },
]

/** The rest of the phone copy a person reads on the same surfaces. */
const ADB_PANE_KEYS: MessageKey[] = [
  'adb.connect.openSettings',
  'adb.readiness.waitingForAuthorization',
  'adb.readiness.offline',
  'adb.readiness.noPermissions',
  'adb.hint.text',
  'adb.hint.how',
  'adb.disconnectDeviceAriaLabel',
  'adb.disconnectBusyTooltip',
]

/**
 * Backend vocabulary that must never surface in phone copy. "adb" and "sync"
 * name the daemon and its service; "transport" and "unauthorized" are the wire's
 * words for things the user experiences as a cable and a prompt.
 */
const ADB_LEAK_WORDS = ['adb', 'transport', 'daemon', 'unauthorized', 'socket']

describe('servers copy obeys the writing rules', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  for (const kind of REFUSAL_KINDS) {
    it(`refusal "${kind}" is clean`, () => {
      const sentence = wordConnectRefusal(kind, { host: 'nas.local', username: 'ada' })
      for (const word of [...NEVER_WORDS, ...TRIVIALIZING_WORDS]) {
        expect(containsWord(sentence, word), `servers.refusal.${kind} contains "${word}": ${sentence}`).toBe(false)
      }
    })
  }

  for (const key of PANE_STATE_KEYS) {
    it(`pane state "${key}" is clean`, () => {
      const sentence = tString(key, PANE_STATE_PARAMS)
      for (const word of [...NEVER_WORDS, ...TRIVIALIZING_WORDS]) {
        expect(containsWord(sentence, word), `${key} contains "${word}": ${sentence}`).toBe(false)
      }
    })
  }

  for (const error of ADB_CONNECT_ERRORS) {
    it(`the phone refusal "${error.type}" is clean`, () => {
      const outcome = readAdbConnectOutcome(error)
      if (outcome.kind === 'silent') return
      const sentences = outcome.kind === 'waiting' ? [outcome.reason, outcome.hint] : [outcome.sentence]
      for (const sentence of sentences) {
        for (const word of [...NEVER_WORDS, ...TRIVIALIZING_WORDS, ...ADB_LEAK_WORDS]) {
          expect(containsWord(sentence, word), `adb.connect.${error.type} contains "${word}": ${sentence}`).toBe(false)
        }
        // ❗ The serial is the one field these variants carry, and it means
        // nothing to a reader. A leak would be a diagnostic in a sentence.
        expect(sentence).not.toContain(ADB_SERIAL)
      }
    })
  }

  for (const key of ADB_PANE_KEYS) {
    it(`the phone's "${key}" is clean`, () => {
      const sentence = tString(key, { name: 'Pixel 7' })
      for (const word of [...NEVER_WORDS, ...TRIVIALIZING_WORDS, ...ADB_LEAK_WORDS]) {
        expect(containsWord(sentence, word), `${key} contains "${word}": ${sentence}`).toBe(false)
      }
    })
  }
})
