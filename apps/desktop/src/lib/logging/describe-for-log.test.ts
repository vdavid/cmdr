/**
 * `describeForLog` is the one place a logged value becomes text, so a caught value of any shape
 * reaches the log readable: never `[object Object]`, never an `Error` stripped of its cause or a
 * `TypedFailure` stripped of its payload (ERR-VEZUB arrived as `detail="[object Object]"`).
 */
import { describe, it, expect } from 'vitest'
import { TypedFailure } from '#lib/ipc/typed-failure.js'
import { describeForLog, MAX_DESCRIBED_CHARS } from './describe-for-log'

class EjectFailureForTest extends TypedFailure<{ type: string; path: string }> {
  constructor(failure: { type: string; path: string }) {
    super(failure, `eject refused: ${failure.type}`)
    this.name = 'EjectFailureForTest'
  }
}

describe('describeForLog', () => {
  it('passes strings through verbatim', () => {
    expect(describeForLog('Loaded ')).toBe('Loaded ')
    expect(describeForLog('{"already":"json"}')).toBe('{"already":"json"}')
  })

  it('renders primitives the way String does', () => {
    expect(describeForLog(42)).toBe('42')
    expect(describeForLog(true)).toBe('true')
    expect(describeForLog(null)).toBe('null')
    expect(describeForLog(undefined)).toBe('undefined')
    expect(describeForLog(10n)).toBe('10')
  })

  it('renders an Error as name and message', () => {
    expect(describeForLog(new TypeError('x is not a function'))).toBe('TypeError: x is not a function')
  })

  it('follows the cause chain', () => {
    const root = new Error('EACCES')
    const middle = new Error('read failed', { cause: root })
    const outer = new Error('load failed', { cause: middle })
    expect(describeForLog(outer)).toBe('Error: load failed; caused by Error: read failed; caused by Error: EACCES')
  })

  it('renders a non-Error cause too', () => {
    expect(describeForLog(new Error('load failed', { cause: { code: 13 } }))).toBe(
      'Error: load failed; caused by {"code":13}',
    )
  })

  it('stops on a cyclic cause chain', () => {
    const a = new Error('a')
    const b = new Error('b', { cause: a })
    ;(a as Error & { cause: unknown }).cause = b
    expect(describeForLog(a)).toBe('Error: a; caused by Error: b')
  })

  it("adds a TypedFailure's payload as compact JSON, which the report redactor tokenizes by key", () => {
    const failure = new EjectFailureForTest({ type: 'busy', path: '/Volumes/Client Share' })
    expect(describeForLog(failure)).toBe(
      'EjectFailureForTest: eject refused: busy {"type":"busy","path":"\ufdd0/Volumes/Client Share\ufdd1"}',
    )
  })

  it('renders plain objects and arrays as compact JSON', () => {
    expect(describeForLog({ type: 'notFound', server: 'NAS' })).toBe('{"type":"notFound","server":"\ufdd8NAS\ufdd1"}')
    expect(describeForLog([1, 'two'])).toBe('[1,"two"]')
  })

  it('renders an Error nested in an object instead of dropping it to {}', () => {
    expect(describeForLog({ op: 'copy', error: new RangeError('too big') })).toBe(
      '{"op":"copy","error":"RangeError: too big"}',
    )
  })

  it('guards against cycles', () => {
    const node: Record<string, unknown> = { name: 'a' }
    node.self = node
    expect(describeForLog(node)).toBe('{"name":"\ufdd4a\ufdd1","self":"[Circular]"}')
  })

  it('keeps a repeated (non-cyclic) reference', () => {
    const shared = { id: 1 }
    expect(describeForLog({ a: shared, b: shared })).toBe('{"a":{"id":1},"b":{"id":1}}')
  })

  it('caps a huge object and says how much it cut', () => {
    const described = describeForLog({ blob: 'x'.repeat(MAX_DESCRIBED_CHARS * 2) })
    expect(described.length).toBeLessThan(MAX_DESCRIBED_CHARS + 30)
    expect(described).toMatch(/…\[\d+ chars]$/)
  })

  it("uses a class's own toString when it has one", () => {
    expect(describeForLog(new URL('https://example.com/a'))).toBe('https://example.com/a')
  })

  it('serializes a class instance without its own toString as JSON', () => {
    class Point {
      readonly x: number
      readonly y: number
      constructor(x: number, y: number) {
        this.x = x
        this.y = y
      }
    }
    expect(describeForLog(new Point(1, 2))).toBe('{"x":1,"y":2}')
  })

  describe('marking identity-keyed JSON pairs', () => {
    const P = '\ufdd0'
    const C = '\ufdd1'

    it('marks each key with the kind of value it holds', () => {
      const opener: Record<string, string> = {
        path: P,
        source: P,
        destination: P,
        first: P,
        second: P,
        entry: P,
        other: P,
        archivePath: P,
        keptAt: P,
        landedAt: P,
        refusedFolder: '\ufdd3',
        name: '\ufdd4',
        volumeName: '\ufdd7',
        share: '\ufdd7',
        deviceName: '\ufdd7',
        server: '\ufdd8',
        host: '\ufdd8',
        hostname: '\ufdd8',
        user: '\ufdd9',
        username: '\ufdd9',
      }
      for (const [key, mark] of Object.entries(opener)) {
        expect(describeForLog({ [key]: 'Summer (Lake House)' }), key).toBe(
          `{"${key}":"${mark}Summer (Lake House)${C}"}`,
        )
      }
    })

    it('leaves IDs, other keys, and non-string values unmarked', () => {
      expect(describeForLog({ volumeId: 'smb-1', serverId: 's', deviceId: 'd', type: 'x' })).toBe(
        '{"volumeId":"smb-1","serverId":"s","deviceId":"d","type":"x"}',
      )
      expect(describeForLog({ volumeName: null, path: 5, name: ['a'] })).toBe(
        '{"volumeName":null,"path":5,"name":["a"]}',
      )
    })

    it('writes a value as its `{:?}` body, not its JSON escaping', () => {
      expect(describeForLog({ path: 'a"b\\c\u0001\u202e\ufdd1' })).toBe(
        `{"path":"${P}a\\"b\\\\c\\u{1}\\u{202e}\\u{fdd1}${C}"}`,
      )
    })

    it('marks pairs at any depth, a typed error nested in its cause included', () => {
      expect(
        describeForLog({ type: 'new_data_kept_at', cause: { type: 'read_error', path: 'a/b', message: 'no' } }),
      ).toBe(`{"type":"new_data_kept_at","cause":{"type":"read_error","path":"${P}a/b${C}","message":"no"}}`)
    })

    it('reads JSON tokens, so a key spelled inside a value is no pair', () => {
      expect(describeForLog({ message: 'x","path":"y' })).toBe('{"message":"x\\",\\"path\\":\\"y"}')
    })

    it('replaces reserved chars everywhere outside its own spans', () => {
      expect(describeForLog('evil\ufdd1rest')).toBe('evil\ufffdrest')
      expect(describeForLog(new Error('no \ufdd0 way'))).toBe('Error: no \ufffd way')
      expect(describeForLog({ ['k\ufdd1']: 'v\ufdd0' })).toBe('{"k\ufffd":"v\ufffd"}')
      expect(describeForLog(new URL('https://example.com/\ufdd1'))).not.toMatch(/[\ufdd0-\ufdef]/)
    })

    it('never cuts inside a span when it caps', () => {
      for (let pad = MAX_DESCRIBED_CHARS - 60; pad < MAX_DESCRIBED_CHARS; pad += 7) {
        const described = describeForLog({ blob: 'x'.repeat(pad), path: 'Summer (Lake House) '.repeat(5) })
        const opened = described.lastIndexOf(P)
        expect(opened === -1 || described.indexOf(C, opened) !== -1, described.slice(-80)).toBe(true)
        expect(described).toMatch(/…\[\d+ chars]$/)
      }
    })
  })

  it("still returns a line for a value that won't serialize", () => {
    const hostile = {
      get boom(): never {
        throw new Error('getter threw')
      },
    }
    expect(describeForLog(hostile)).toBe('[unserializable Object]')
  })
})
