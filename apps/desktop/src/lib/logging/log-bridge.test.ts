import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { LogRecord } from '@logtape/logtape'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

import { invoke } from '@tauri-apps/api/core'
import { TypedFailure } from '#lib/ipc/typed-failure.js'
import cases from './bridge-redaction-cases.json'
import { getTauriBridgeSink, stopBridge } from './log-bridge'

// The markers `log-markers.json` assigns, spelled out so a test reads the bytes it pins.
const P = '\ufdd0'
const C = '\ufdd1'
const DIR_PATH = '\ufdd3'
const NAME = '\ufdd4'
const FILE_NAME = '\ufdd5'
const DIR_NAME = '\ufdd6'
const VOLUME = '\ufdd7'
const HOST = '\ufdd8'

class EjectFailureForTest extends TypedFailure<{ type: string; path: string }> {
  constructor(failure: { type: string; path: string }) {
    super(failure, `eject refused: ${failure.type}`)
    this.name = 'EjectFailureForTest'
  }
}

function makeRecord(overrides: Partial<LogRecord> = {}): LogRecord {
  return {
    level: 'info',
    category: ['app', 'test'],
    message: ['test message'],
    rawMessage: 'test message',
    properties: {},
    timestamp: Date.now(),
    ...overrides,
  }
}

describe('log-bridge', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    stopBridge()
  })

  afterEach(() => {
    stopBridge()
    vi.useRealTimers()
  })

  it('batches entries and flushes after 100ms', async () => {
    const sink = getTauriBridgeSink()
    sink(makeRecord())

    expect(invoke).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(100)

    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
      entries: [{ level: 'info', category: 'test', message: 'test message' }],
    })
  })

  it('deduplicates identical consecutive messages', async () => {
    const sink = getTauriBridgeSink()
    const record = makeRecord()

    for (let i = 0; i < 5; i++) {
      sink(record)
    }

    await vi.advanceTimersByTimeAsync(100)

    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
      entries: [{ level: 'info', category: 'test', message: 'test message (×5, deduplicated)' }],
    })
  })

  it('does not deduplicate non-consecutive identical messages', async () => {
    const sink = getTauriBridgeSink()
    const recordA = makeRecord({ message: ['message A'] })
    const recordB = makeRecord({ message: ['message B'] })

    sink(recordA)
    sink(recordB)
    sink(recordA)

    await vi.advanceTimersByTimeAsync(100)

    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
      entries: [
        { level: 'info', category: 'test', message: 'message A' },
        { level: 'info', category: 'test', message: 'message B' },
        { level: 'info', category: 'test', message: 'message A' },
      ],
    })
  })

  it('throttles at 200/s and emits warning', async () => {
    const sink = getTauriBridgeSink()

    for (let i = 0; i < 210; i++) {
      sink(makeRecord({ message: [`msg ${String(i)}`] }))
    }

    await vi.advanceTimersByTimeAsync(100)

    expect(invoke).toHaveBeenCalledOnce()

    const callArgs = vi.mocked(invoke).mock.calls[0] as [
      string,
      { entries: { level: string; category: string; message: string }[] },
    ]
    const entries = callArgs[1].entries

    // 200 regular entries + 1 throttle warning
    expect(entries).toHaveLength(201)

    // The warning entry should mention dropped entries
    const warningEntry = entries.find((e) => e.category === 'log-bridge')
    expect(warningEntry).toBeDefined()
    expect(warningEntry?.level).toBe('warn')
    expect(warningEntry?.message).toContain('entries dropped in the last second')
  })

  it('throttle warning names the categories that dominated the dropped entries', async () => {
    const sink = getTauriBridgeSink()

    // Fill the per-second budget with one category, then overflow with two others.
    for (let i = 0; i < 200; i++) {
      sink(makeRecord({ category: ['app', 'filler'], message: [`msg ${String(i)}`] }))
    }
    for (let i = 0; i < 30; i++) {
      sink(makeRecord({ category: ['app', 'fileExplorer'], message: [`dropped a ${String(i)}`] }))
    }
    for (let i = 0; i < 10; i++) {
      sink(makeRecord({ category: ['app', 'search'], message: [`dropped b ${String(i)}`] }))
    }

    await vi.advanceTimersByTimeAsync(100)

    const callArgs = vi.mocked(invoke).mock.calls[0] as [
      string,
      { entries: { level: string; category: string; message: string }[] },
    ]
    const warningEntry = callArgs[1].entries.find((e) => e.category === 'log-bridge')
    expect(warningEntry?.message).toContain('40 entries dropped')
    expect(warningEntry?.message).toContain('fileExplorer ×30')
    expect(warningEntry?.message).toContain('search ×10')
  })

  it('sends nothing when buffer is empty', async () => {
    await vi.advanceTimersByTimeAsync(200)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('maps LogTape "warning" level to "warn"', async () => {
    const sink = getTauriBridgeSink()
    sink(makeRecord({ level: 'warning' }))

    await vi.advanceTimersByTimeAsync(100)

    expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
      entries: [{ level: 'warn', category: 'test', message: 'test message' }],
    })
  })

  it('strips "app" prefix from category', async () => {
    const sink = getTauriBridgeSink()
    sink(makeRecord({ category: ['app', 'fileExplorer'] }))

    await vi.advanceTimersByTimeAsync(100)

    const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { category: string }[] }]
    expect(callArgs[1].entries[0].category).toBe('fileExplorer')
  })

  it('joins multi-part message arrays into a string', async () => {
    const sink = getTauriBridgeSink()
    sink(makeRecord({ message: ['Loaded ', 42, ' items'] }))

    await vi.advanceTimersByTimeAsync(100)

    const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { message: string }[] }]
    expect(callArgs[1].entries[0].message).toBe('Loaded 42 items')
  })

  async function renderedMessage(record: Partial<LogRecord>): Promise<string> {
    getTauriBridgeSink()(makeRecord(record))
    await vi.advanceTimersByTimeAsync(100)
    const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { message: string }[] }]
    return callArgs[1].entries[0].message
  }

  it('renders path and identity placeholders as keyed, Debug-quoted, marked fields', async () => {
    const message = await renderedMessage({
      rawMessage: 'Listing {listingId} on {volumeId} failed for {path} (tries: {count}): {error}',
      message: [
        'Listing ',
        'l-1',
        ' on ',
        'smb-nas-0123456789abcdef',
        ' failed for ',
        'docs/Client "Plans"\\2026\nnext',
        ' (tries: ',
        3,
        '): ',
        'boom',
        '',
      ],
    })
    // A volume ID has no marker kind: it stays a quoted field the redactor keys on.
    expect(message).toBe(
      `Listing l-1 on volumeId="smb-nas-0123456789abcdef" failed for path="${P}docs/Client \\"Plans\\"\\\\2026\\nnext${C}" (tries: 3): detail="boom"`,
    )
  })

  it('does not repeat a key the template already spells out', async () => {
    const message = await renderedMessage({
      rawMessage: 'calling listDirectoryStart: volumeId={volumeId}, path={loadPath}',
      message: ['calling listDirectoryStart: volumeId=', 'root', ', path=', '/Users/test_user_1/Plans', ''],
    })
    expect(message).toBe(`calling listDirectoryStart: volumeId="root", path="${DIR_PATH}/Users/test_user_1/Plans${C}"`)
  })

  it('marks each placeholder with the kind its name says', async () => {
    const cases: [placeholder: string, key: string, opener: string][] = [
      ['path', 'path', P],
      ['input', 'input', P],
      ['destinationPath', 'destination', P],
      ['loadPath', 'path', DIR_PATH],
      ['landedPath', 'path', DIR_PATH],
      ['root', 'path', DIR_PATH],
      ['target', 'path', DIR_PATH],
      ['dir', 'dir', DIR_PATH],
      ['folder', 'dir', DIR_PATH],
      ['folderName', 'dir', DIR_NAME],
      ['name', 'file', NAME],
      ['fileName', 'file', FILE_NAME],
      ['volumeName', 'volumeName', VOLUME],
      ['share', 'share', VOLUME],
      ['host', 'host', HOST],
      ['server', 'server', HOST],
    ]
    for (const [placeholder, key, opener] of cases) {
      vi.mocked(invoke).mockClear()
      expect(
        await renderedMessage({ rawMessage: `at {${placeholder}}`, message: ['at ', 'Summer (Lake House)', ''] }),
        placeholder,
      ).toBe(`at ${key}="${opener}Summer (Lake House)${C}"`)
    }
  })

  it("escapes a hostile name's own markers, raw or as text, so its span can't close early", async () => {
    expect(
      await renderedMessage({
        rawMessage: 'Rename {name} refused',
        message: ['Rename ', 'evil\ufdd1rest \\u{fdd1} tail', ' refused'],
      }),
    ).toBe(`Rename file="${NAME}evil\\u{fdd1}rest \\\\u{fdd1} tail${C}" refused`)
  })

  it('replaces reserved chars in text outside its spans, so foreign text never forges a marker', async () => {
    expect(
      await renderedMessage({
        rawMessage: '{op} refused: {error}',
        message: ['', 'Re\ufdd0name', ' refused: ', new Error('cannot open evil\ufdd1rest'), ''],
      }),
    ).toBe('Re\ufffdname refused: detail="Error: cannot open evil\ufffdrest"')
  })

  it("keeps a detail's own spans one depth down, their values escaped twice", async () => {
    const failure = new EjectFailureForTest({ type: 'busy', path: '/Volumes/A "B"\\c' })
    // describeForLog writes `"path":"<P>/Volumes/A \"B\"\\c<C>"`, which the detail quotes once more.
    expect(
      await renderedMessage({ rawMessage: 'Eject failed: {error}', message: ['Eject failed: ', failure, ''] }),
    ).toBe(
      'Eject failed: detail="EjectFailureForTest: eject refused: busy {\\"type\\":\\"busy\\",\\"path\\":\\"\\u{fdd0}/Volumes/A \\\\\\"B\\\\\\"\\\\\\\\c\\u{fdd1}\\"}"',
    )
  })

  it('keeps the dedup suffix outside every span', async () => {
    const sink = getTauriBridgeSink()
    const record = makeRecord({ rawMessage: 'at {path}', message: ['at ', 'a/b', ''] })
    sink(record)
    sink(record)
    await vi.advanceTimersByTimeAsync(100)
    const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { message: string }[] }]
    expect(callArgs[1].entries[0].message).toBe(`at path="${P}a/b${C}" (×2, deduplicated)`)
  })

  it('produces exactly the lines the Rust redaction cells read', async () => {
    // `redact/fe_bridge_tests.rs` feeds these same lines to the report redactor.
    for (const testCase of cases.lines) {
      vi.mocked(invoke).mockClear()
      const names = [...testCase.rawMessage.matchAll(/\{([^{}\s]+)\}/g)].map((m) => m[1])
      const literals = testCase.rawMessage.split(/\{[^{}\s]+\}/)
      const values = testCase.values as Record<string, unknown>
      const message = literals.flatMap((literal, index) =>
        index < names.length ? [literal, values[names[index]]] : [literal],
      )
      expect(await renderedMessage({ rawMessage: testCase.rawMessage, message }), testCase.case).toBe(testCase.line)
    }
  })

  it('renders error, err, and result values as detail fields', async () => {
    expect(
      await renderedMessage({
        rawMessage: 'Mount failed: {error}; retry: {err}; answer: {result}',
        message: ['Mount failed: ', '{"server":"NAS"}', '; retry: ', 'boom', '; answer: ', '{"ok":true}', ''],
      }),
    ).toBe('Mount failed: detail="{\\"server\\":\\"NAS\\"}"; retry: detail="boom"; answer: detail="{\\"ok\\":true}"')
  })

  it('maps names to the redactor vocabulary and leaves a template array as plain text', async () => {
    expect(
      await renderedMessage({
        rawMessage: 'Volume not found: {volumeName} on {server}, file {name}',
        message: ['Volume not found: ', 'Client Share', ' on ', 'nas', ', file ', 'a.pdf', ''],
      }),
    ).toBe(
      `Volume not found: volumeName="${VOLUME}Client Share${C}" on server="${HOST}nas${C}", file file="${NAME}a.pdf${C}"`,
    )

    vi.mocked(invoke).mockClear()
    expect(
      await renderedMessage({
        rawMessage: 'Unhandled promise rejection: {detail}',
        message: ['Unhandled promise rejection: ', 'Error: denied /srv/x', ''],
      }),
    ).toBe('Unhandled promise rejection: detail="Error: denied /srv/x"')

    vi.mocked(invoke).mockClear()
    const template = Object.assign(['Loaded ', ' items'], { raw: ['Loaded ', ' items'] })
    expect(await renderedMessage({ rawMessage: template, message: ['Loaded ', 42, ' items'] })).toBe('Loaded 42 items')
  })

  it('renders a raw caught value readably instead of as [object Object]', async () => {
    // ERR-VEZUB arrived as `detail="[object Object]"`: the bridge stringified with `String(part)`.
    const failure = { type: 'notFound', path: '/Users/test_user_1/Plans' }
    expect(
      await renderedMessage({
        rawMessage: 'Eject failed: {error}',
        message: ['Eject failed: ', failure, ''],
      }),
    ).toBe(
      'Eject failed: detail="{\\"type\\":\\"notFound\\",\\"path\\":\\"\\u{fdd0}/Users/test_user_1/Plans\\u{fdd1}\\"}"',
    )

    vi.mocked(invoke).mockClear()
    expect(
      await renderedMessage({
        rawMessage: 'Step {step} threw',
        message: ['Step ', new Error('outer', { cause: new TypeError('inner') }), ' threw'],
      }),
    ).toBe('Step Error: outer; caused by TypeError: inner threw')

    vi.mocked(invoke).mockClear()
    const template = Object.assign(['Dock pin failed: ', ''], { raw: ['Dock pin failed: ', ''] })
    expect(await renderedMessage({ rawMessage: template, message: ['Dock pin failed: ', { code: 5 }, ''] })).toBe(
      'Dock pin failed: {"code":5}',
    )
  })

  it('silently drops entries when invoke fails', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('Backend unavailable'))

    const sink = getTauriBridgeSink()
    sink(makeRecord())

    // Should not throw
    await vi.advanceTimersByTimeAsync(100)
  })
})
