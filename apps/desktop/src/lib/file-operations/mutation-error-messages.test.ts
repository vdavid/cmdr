/**
 * Every typed refusal a mutation can ship has words, in every locale that ships.
 *
 * The switch statements are exhaustive at the type level, so what these tests
 * catch is the other half: a variant whose catalog key was never added (which
 * renders the key itself, or an empty string) and a message that breaks the
 * error-copy writing rules. Both fail silently at runtime.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import type { MutationError, VolumeError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { renderMutationError, renderVolumeError, technicalDetail } from './mutation-error-messages'
import { MutationFailure, asMutationError, isMutationTimeout, throwMutationError } from './mutation-error'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** One value per `VolumeError` variant. Adding a variant makes this list fail to typecheck. */
const VOLUME_CASES: VolumeError[] = [
  { type: 'notFound', data: '/Volumes/share/holiday.raw' },
  { type: 'permissionDenied', data: '/Volumes/share/private' },
  { type: 'alreadyExists', data: '/Volumes/share/notes.txt' },
  { type: 'notSupported' },
  { type: 'deviceDisconnected', data: 'the phone went away' },
  { type: 'deviceSessionReset', data: 'PTP DeviceReset' },
  { type: 'readOnly', data: 'mounted read-only' },
  { type: 'storageFull', data: { message: 'STATUS_DISK_FULL' } },
  { type: 'connectionTimeout', data: 'no reply in 10s' },
  { type: 'cancelled', data: 'Operation cancelled by user' },
  { type: 'isADirectory', data: '/Volumes/share/album' },
  { type: 'invalidName', data: 'STATUS_OBJECT_NAME_INVALID' },
  { type: 'deletePending', data: '/Volumes/share/doomed.txt' },
  { type: 'staleDestinationHandle', data: '/DCIM/Camera' },
  { type: 'ioError', data: { message: 'input/output error', rawOsError: 5 } },
  { type: 'needsPassword', data: { wrongAttempt: false } },
  { type: 'needsPassword', data: { wrongAttempt: true } },
  { type: 'friendlyGit', data: { kind: 'notARepo', path: '/repo', raw: null } },
]

/** One value per `MutationError` variant, in declaration order. */
const MUTATION_CASES: MutationError[] = [
  { type: 'nameEmpty' },
  { type: 'nameHasDisallowedCharacter' },
  { type: 'notFound', path: '/Users/me/gone.txt' },
  { type: 'cantRenameVolumeRoot' },
  { type: 'parentNotWritable', path: '/Users/me/locked' },
  { type: 'fileLocked', path: '/Users/me/locked.txt' },
  { type: 'sipProtected', path: '/System/Library' },
  { type: 'volumeGone', volumeId: 'smb-naspi-papers' },
  { type: 'archiveNotEditable' },
  { type: 'archiveReadOnly' },
  { type: 'renameOutOfArchive' },
  { type: 'renameAcrossArchives' },
  { type: 'archiveEditNotReady' },
  { type: 'archiveEditCouldntStart', detail: 'SinkMissing' },
  { type: 'alreadyExists', name: 'notes.txt' },
  { type: 'volume', error: { type: 'notFound', data: '/Volumes/share/holiday.raw' } },
  { type: 'timedOut' },
  { type: 'unexpected', detail: "the rename task didn't finish: panicked" },
]

/** The writing rules for error copy (`docs/guides/error-handling.md`). */
function assertErrorCopyRules(message: string, label: string): void {
  expect(message, `${label} must have words`).not.toBe('')
  expect(message, `${label} must resolve, not echo its key`).not.toMatch(/^errors\./)
  expect(message.toLowerCase(), `${label} must not say "error"`).not.toMatch(/\berror\b/)
  expect(message.toLowerCase(), `${label} must not say "failed"`).not.toMatch(/\bfailed\b/)
  for (const trivializer of ['just ', 'simply', 'simple ', 'easy ']) {
    expect(message.toLowerCase(), `${label} must not trivialize with "${trivializer.trim()}"`).not.toContain(
      trivializer,
    )
  }
  expect(message, `${label} must not leave a placeholder unfilled`).not.toMatch(/\{[a-zA-Z]+\}/)
}

describe('renderVolumeError', () => {
  for (const error of VOLUME_CASES) {
    const label =
      error.type === 'needsPassword' ? `needsPassword(wrong=${String(error.data.wrongAttempt)})` : error.type
    it(`words ${label}`, () => {
      assertErrorCopyRules(renderVolumeError(error), `volume ${label}`)
    })
  }

  it('names the path it was handed, which is what the user is looking for', () => {
    expect(renderVolumeError({ type: 'notFound', data: '/Volumes/share/holiday.raw' })).toContain(
      '/Volumes/share/holiday.raw',
    )
  })

  it('keeps a git failure speaking git rather than flattening it', () => {
    const git = renderVolumeError({ type: 'friendlyGit', data: { kind: 'notARepo', path: '/x', raw: null } })
    expect(git).toContain('git')
  })
})

describe('renderMutationError', () => {
  for (const error of MUTATION_CASES) {
    it(`words ${error.type}`, () => {
      assertErrorCopyRules(renderMutationError(error), `mutation ${error.type}`)
      assertErrorCopyRules(renderMutationError(error, 'folder'), `mutation ${error.type} (folder)`)
    })
  }

  it('quotes the name the user typed when it is taken, not the whole path', () => {
    expect(renderMutationError({ type: 'alreadyExists', name: 'notes.txt' })).toContain('"notes.txt"')
  })

  it('agrees with the live validation, so a turned-down name reads the way the red border read', () => {
    expect(renderMutationError({ type: 'nameEmpty' }, 'folder')).toBe("Folder name can't be empty")
    expect(renderMutationError({ type: 'nameEmpty' }, 'file')).toBe("Filename can't be empty")
  })

  it("says a timeout may still land, because the backend's deadline detaches rather than cancels", () => {
    expect(renderMutationError({ type: 'timedOut' }).toLowerCase()).toContain('may still')
  })

  it('never renders the untranslated detail as the message', () => {
    const rendered = renderMutationError({ type: 'unexpected', detail: 'a Rust panic nobody should read' })
    expect(rendered).not.toContain('Rust panic')
  })
})

describe('technicalDetail', () => {
  it('hands back the backend diagnostic for the fallback', () => {
    expect(technicalDetail({ type: 'unexpected', detail: 'join failure' })).toBe('join failure')
  })

  it('spells out the errno beside an I/O diagnostic', () => {
    expect(
      technicalDetail({ type: 'volume', error: { type: 'ioError', data: { message: 'i/o', rawOsError: 5 } } }),
    ).toBe('i/o (errno 5)')
  })

  it('has nothing to add for a refusal that only carries a path', () => {
    expect(technicalDetail({ type: 'notFound', path: '/gone' })).toBeNull()
    expect(technicalDetail({ type: 'volume', error: { type: 'notFound', data: '/gone' } })).toBeNull()
  })
})

describe('MutationFailure', () => {
  it('survives the throw with its typed value intact', () => {
    try {
      throwMutationError({ type: 'alreadyExists', name: 'notes.txt' })
    } catch (e) {
      expect(asMutationError(e)).toEqual({ type: 'alreadyExists', name: 'notes.txt' })
      expect(e).toBeInstanceOf(Error)
    }
  })

  it('reports a timeout without anyone reading a sentence', () => {
    expect(isMutationTimeout(new MutationFailure({ type: 'timedOut' }))).toBe(true)
    expect(isMutationTimeout(new MutationFailure({ type: 'nameEmpty' }))).toBe(false)
    expect(isMutationTimeout(new Error('something else'))).toBe(false)
  })
})
