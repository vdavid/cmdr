/**
 * Every typed refusal an eject or a disconnect can ship has words, in every
 * locale that ships.
 *
 * The message table is exhaustive at the type level, so what these tests catch
 * is the other half: a variant whose catalog key was never added (which renders
 * the key itself, or an empty string), a message that breaks the error-copy
 * writing rules, and — the one that used to reach users — `diskutil`'s raw
 * English stderr leaking into the sentence a person reads.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach, vi } from 'vitest'
import type { EjectError, HolderKind, HolderScan, VolumeHolder } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { renderEjectError, ejectTechnicalDetail, wordEjectRefusal } from './eject-error-messages'
import { EjectFailure, asEjectError, throwEjectError } from './eject-error'

// One stable spy, so the Cmdr-holder warn can be asserted on.
const { warn } = vi.hoisted(() => ({ warn: vi.fn() }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/**
 * A refusal nothing could scan: the shape that says "nobody could be named", never "nobody is
 * holding it". It words the same unnamed sentence a complete scan with no names does.
 */
const NOBODY_NAMED: HolderScan = { type: 'incomplete', named: [] }

/** One value per `EjectError` variant, in declaration order. Adding a variant makes this list fail to typecheck. */
const EJECT_CASES: EjectError[] = [
  { type: 'busy' },
  { type: 'volumeNotFound', volumeId: 'volumes-usb-drive' },
  { type: 'notEjectable', volumeId: 'root' },
  { type: 'notAnSmbVolume', volumeId: 'volumes-usb-drive' },
  { type: 'deviceDisconnectRefused', provider: 'mtp', detail: 'PTP CloseSession timed out' },
  {
    type: 'unmountRefused',
    holders: NOBODY_NAMED,
    detail: 'Unmount failed for /Volumes/Trip: in use by process 1234 (mds)',
  },
  { type: 'timedOut' },
  { type: 'notResponding', step: 'indexStop' },
  { type: 'unexpected', detail: 'the eject task panicked' },
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

describe('renderEjectError', () => {
  for (const error of EJECT_CASES) {
    it(`words ${error.type}`, () => {
      assertErrorCopyRules(renderEjectError(error), `eject ${error.type}`)
    })
  }

  it("says a timeout may still land, because the backend's deadline detaches rather than cancels", () => {
    expect(renderEjectError({ type: 'timedOut' }).toLowerCase()).toContain('may still')
  })

  it("doesn't promise a stalled eject may still land, because nothing was unmounted", () => {
    // The deadline passed before the unmount ran (an index stop or an ejectability
    // check that hung), so `timedOut`'s "may still eject on its own" would be false.
    const rendered = renderEjectError({ type: 'notResponding', step: 'ejectabilityCheck' })
    expect(rendered).not.toBe(renderEjectError({ type: 'timedOut' }))
    expect(rendered.toLowerCase()).not.toContain('may still')
  })

  it('never renders the untranslated OS text as the message', () => {
    const rendered = renderEjectError({
      type: 'unmountRefused',
      holders: NOBODY_NAMED,
      detail: 'Unmount failed for /Volumes/Trip: in use by process 1234 (mds)',
    })
    expect(rendered).not.toContain('mds')
    expect(rendered).not.toContain('/Volumes/Trip')
  })

  it('tells a busy drive apart from a drive the OS refused, which used to read the same', () => {
    expect(renderEjectError({ type: 'busy' })).not.toBe(
      renderEjectError({ type: 'unmountRefused', holders: NOBODY_NAMED, detail: 'x' }),
    )
  })
})

describe('ejectTechnicalDetail', () => {
  it("hands back the OS's own words, which often name the process holding the drive", () => {
    expect(
      ejectTechnicalDetail({ type: 'unmountRefused', holders: NOBODY_NAMED, detail: 'in use by process 1234 (mds)' }),
    ).toBe('in use by process 1234 (mds)')
  })

  it('has nothing to add for a refusal that carries no diagnostic', () => {
    expect(ejectTechnicalDetail({ type: 'busy' })).toBeNull()
    expect(ejectTechnicalDetail({ type: 'notEjectable', volumeId: 'root' })).toBeNull()
  })
})

describe('EjectFailure', () => {
  it('survives the throw with its typed value intact', () => {
    try {
      throwEjectError({ type: 'unmountRefused', holders: NOBODY_NAMED, detail: 'in use by process 1234 (mds)' })
      expect.unreachable('throwEjectError must throw')
    } catch (e) {
      expect(asEjectError(e)).toEqual({
        type: 'unmountRefused',
        holders: NOBODY_NAMED,
        detail: 'in use by process 1234 (mds)',
      })
      expect(e).toBeInstanceOf(Error)
    }
  })

  it('is not mistaken for some other error', () => {
    expect(asEjectError(new Error('something else'))).toBeNull()
    expect(asEjectError('a string')).toBeNull()
  })
})

describe('wordEjectRefusal', () => {
  it('words a typed refusal from the catalog', () => {
    expect(wordEjectRefusal(new EjectFailure({ type: 'busy' }))).toBe(renderEjectError({ type: 'busy' }))
  })

  it('falls back honestly when the transport itself broke, without showing the raw value', () => {
    const rendered = wordEjectRefusal(new Error('IPC channel closed'))
    expect(rendered).toBe(renderEjectError({ type: 'unexpected', detail: '' }))
    expect(rendered).not.toContain('IPC channel')
  })
})

/** One holder, named and classified. `pid` only has to be unique within a case. */
function holder(kind: HolderKind, name: string, pid = 1000 + name.length): VolumeHolder {
  return { pid, name, bundleId: kind === 'app' ? `com.example.${name.toLowerCase()}` : null, kind }
}

/** The refusal a set of holders produces, as a complete scan. */
function refusedBy(named: VolumeHolder[]): EjectError {
  return { type: 'unmountRefused', holders: { type: 'complete', named }, detail: 'diskutil said no' }
}

describe('a refused unmount names who held the drive', () => {
  it('names the one app, so the person knows what to close', () => {
    expect(renderEjectError(refusedBy([holder('app', 'Preview')]))).toBe(
      'Preview still has files open there. Close them, then try again.',
    )
  })

  it('joins two names with "and"', () => {
    expect(renderEjectError(refusedBy([holder('app', 'Preview'), holder('app', 'Warp')]))).toBe(
      'Preview and Warp still have files open there. Close them, then try again.',
    )
  })

  it('joins three names as a list', () => {
    const rendered = renderEjectError(
      refusedBy([holder('app', 'Preview'), holder('app', 'Warp'), holder('app', 'Photos')]),
    )
    expect(rendered).toBe('Preview, Warp, and Photos still have files open there. Close them, then try again.')
  })

  it('stops at three names and says "other apps" for the rest, so the toast stays one line', () => {
    const four = [holder('app', 'Preview'), holder('app', 'Warp'), holder('app', 'Photos'), holder('app', 'Music')]
    expect(renderEjectError(refusedBy(four))).toBe(
      'Preview, Warp, Photos, and other apps still have files open there. Close them, then try again.',
    )
    const six = [...four, holder('app', 'Mail'), holder('app', 'Notes')]
    expect(renderEjectError(refusedBy(six))).toBe(renderEjectError(refusedBy(four)))
  })

  it('words a tool exactly like an app, because a name is a name to the person reading it', () => {
    expect(renderEjectError(refusedBy([holder('tool', 'mds_stores')]))).toBe(
      'mds_stores still has files open there. Close them, then try again.',
    )
  })

  it('counts two processes of one app once, so a helper-heavy app reads as one name', () => {
    const twoOfOne = [holder('app', 'Warp', 101), holder('app', 'Warp', 102)]
    expect(renderEjectError(refusedBy(twoOfOne))).toBe(renderEjectError(refusedBy([holder('app', 'Warp', 101)])))
  })

  it('sends someone to the disk image first, since the drive can’t go before it does', () => {
    expect(renderEjectError(refusedBy([holder('diskImage', 'Installer')]))).toBe(
      'A disk image stored there is still open. Eject that image first, then try again.',
    )
  })

  it('tells someone to wait when macOS itself is the holder, because there is nothing to close', () => {
    expect(renderEjectError(refusedBy([holder('system', 'mds_stores')]))).toBe(
      'macOS still has files open there. Wait a minute, then try again.',
    )
  })

  it('owns it when Cmdr is the holder, and invites a report', () => {
    expect(renderEjectError(refusedBy([holder('cmdr', 'cmdr')]))).toBe(
      'Cmdr itself still has files open there. Wait a moment and try again, or send a report if it keeps happening.',
    )
  })

  it('words the app a person can act on when kinds are mixed', () => {
    const mixed = [holder('system', 'mds_stores'), holder('app', 'Preview'), holder('cmdr', 'cmdr')]
    expect(renderEjectError(refusedBy(mixed))).toBe(renderEjectError(refusedBy([holder('app', 'Preview')])))
  })

  it('falls back to the unnamed sentence when the scan named nobody', () => {
    expect(renderEjectError(refusedBy([]))).toBe(
      'Something still has files open there. Close any open files and apps, then try again.',
    )
  })

  it('names the one process when nothing said what kind it is, and says it is only a process name', () => {
    expect(renderEjectError(refusedBy([holder('unclassified', 'mdworker')]))).toBe(
      'A process called mdworker still has files open there. (Cmdr doesn’t have an app name, only the process name.)',
    )
  })

  it('names several unclassified processes with their count, and never calls them apps', () => {
    const rendered = renderEjectError(
      refusedBy([holder('unclassified', 'some-helper'), holder('unclassified', 'mdworker')]),
    )
    expect(rendered).toBe(
      '2 processes called some-helper and mdworker still have files open there. (Cmdr doesn’t have app names, only the process names.)',
    )
    expect(rendered).not.toContain('apps still have')
  })

  it('counts two processes of one name once, like an app', () => {
    const twoOfOne = [holder('unclassified', 'mdworker', 101), holder('unclassified', 'mdworker', 102)]
    expect(renderEjectError(refusedBy(twoOfOne))).toBe(
      renderEjectError(refusedBy([holder('unclassified', 'mdworker', 101)])),
    )
  })

  it('stops at three process names and says "others" for the rest, counting them all', () => {
    const five = ['a-helper', 'b-helper', 'c-helper', 'd-helper', 'e-helper'].map((n) => holder('unclassified', n))
    expect(renderEjectError(refusedBy(five))).toBe(
      '5 processes called a-helper, b-helper, c-helper, and others still have files open there. (Cmdr doesn’t have app names, only the process names.)',
    )
  })

  it('still prefers macOS, Cmdr, or a disk image over an unclassified name', () => {
    const mixed = [holder('unclassified', 'mdworker'), holder('system', 'mds_stores')]
    expect(renderEjectError(refusedBy(mixed))).toBe(renderEjectError(refusedBy([holder('system', 'mds_stores')])))
  })

  it('words an incomplete scan from the names it did see', () => {
    const partial: HolderScan = { type: 'incomplete', named: [holder('app', 'Preview')] }
    expect(renderEjectError({ type: 'unmountRefused', holders: partial, detail: 'x' })).toBe(
      renderEjectError(refusedBy([holder('app', 'Preview')])),
    )
  })

  it("never words a scan that couldn't finish as a drive nothing is using", () => {
    // ❗ The one thing this copy may not say. `incomplete` with no names means
    // Cmdr couldn't tell, which is not the same as nobody holding the drive.
    const rendered = renderEjectError({ type: 'unmountRefused', holders: NOBODY_NAMED, detail: 'x' })
    expect(rendered).toBe(renderEjectError(refusedBy([])))
    expect(rendered.toLowerCase()).not.toContain('nothing is using')
    expect(rendered.toLowerCase()).not.toContain('no app')
  })

  for (const named of [
    [holder('app', 'Preview')],
    [holder('app', 'Preview'), holder('tool', 'rsync')],
    [holder('diskImage', 'Installer')],
    [holder('system', 'mds_stores')],
    [holder('cmdr', 'cmdr')],
    [holder('unclassified', 'some-helper')],
  ]) {
    it(`keeps the error-copy rules for a refusal held by ${named.map((h) => h.kind).join(' + ')}`, () => {
      assertErrorCopyRules(renderEjectError(refusedBy(named)), `refusal held by ${named[0].kind}`)
    })
  }
})

describe('a Cmdr holder is logged as a bug', () => {
  beforeEach(() => {
    warn.mockClear()
  })

  it('warns when Cmdr holds the drive, even though the app got the sentence', () => {
    wordEjectRefusal(new EjectFailure(refusedBy([holder('app', 'Preview'), holder('cmdr', 'cmdr')])))
    expect(warn.mock.calls.some(([template]) => String(template).includes('Cmdr'))).toBe(true)
  })

  it('stays quiet about Cmdr when no Cmdr holder is in the list', () => {
    wordEjectRefusal(new EjectFailure(refusedBy([holder('app', 'Preview')])))
    expect(warn.mock.calls.some(([template]) => String(template).includes('Cmdr'))).toBe(false)
  })
})
