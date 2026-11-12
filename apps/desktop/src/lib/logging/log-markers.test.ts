/**
 * The frontend half of the log markers: the same chars and the same `{:?}` body escaping as
 * `cmdr_fs::log_private`, so a value hashes the same in a report whichever side logged it. The
 * Rust side pins both against `bridge-redaction-cases.json` (`redact/fe_bridge_tests.rs`).
 */
import { describe, it, expect } from 'vitest'
import cases from './bridge-redaction-cases.json'
import {
  LOG_MARKER_CLOSER,
  debugBody,
  debugQuote,
  isReservedChar,
  markLogValue,
  stripLogMarkers,
  withoutReserved,
} from './log-markers'

describe('debugBody', () => {
  it("escapes what Rust's `{:?}` escapes in the safety set, the way Rust spells it", () => {
    expect(debugBody('a"b\\c')).toBe('a\\"b\\\\c')
    expect(debugBody('\n\r\t\0')).toBe('\\n\\r\\t\\0')
    expect(debugBody('\u0001\u007f\u0085')).toBe('\\u{1}\\u{7f}\\u{85}')
    expect(debugBody('a\u202eb\u2066\u061c\u200f')).toBe('a\\u{202e}b\\u{2066}\\u{61c}\\u{200f}')
    expect(debugBody('\u2028\u2029')).toBe('\\u{2028}\\u{2029}')
    expect(debugBody('evil\ufdd1rest\ufdef')).toBe('evil\\u{fdd1}rest\\u{fdef}')
  })

  it('leaves everything else raw, combining marks included', () => {
    expect(debugBody('Café Cafe\u0301 雪 🙂')).toBe('Café Cafe\u0301 雪 🙂')
  })

  it('matches the probes the Rust side checks against its own `{:?}`', () => {
    for (const probe of cases.bodyProbes) {
      expect(debugBody(probe.value), JSON.stringify(probe.value)).toBe(probe.body)
    }
  })

  it('quotes with the same body', () => {
    expect(debugQuote('x"\ufdd1')).toBe('"x\\"\\u{fdd1}"')
  })
})

describe('markLogValue', () => {
  it('wraps the body between the kind opener and the closer', () => {
    expect(markLogValue('path', 'a/b')).toBe('\ufdd0a/b\ufdd1')
    expect(markLogValue('file_path', '/x.pdf')).toBe('\ufdd2/x.pdf\ufdd1')
    expect(markLogValue('user', 'test_user_1')).toBe('\ufdd9test_user_1\ufdd1')
    expect(LOG_MARKER_CLOSER).toBe('\ufdd1')
  })

  it("escapes a hostile name's own markers so they can't close the span", () => {
    expect(markLogValue('name', 'a\ufdd1b\\u{fdd1}')).toBe('\ufdd4a\\u{fdd1}b\\\\u{fdd1}\ufdd1')
  })
})

describe('withoutReserved', () => {
  it('replaces every reserved char, and nothing else', () => {
    expect(withoutReserved('a\ufdd0b\ufdd1c\ufdefd\ufdcf')).toBe('a\ufffdb\ufffdc\ufffdd\ufdcf')
    const plain = 'nothing to see'
    expect(withoutReserved(plain)).toBe(plain)
  })
})

describe('isReservedChar', () => {
  it('knows the range', () => {
    expect(isReservedChar('\ufdd0')).toBe(true)
    expect(isReservedChar('\ufdef')).toBe(true)
    expect(isReservedChar('\ufdf0')).toBe(false)
    expect(isReservedChar('a')).toBe(false)
  })
})

describe('stripLogMarkers', () => {
  it('drops raw markers', () => {
    expect(stripLogMarkers('open path="\ufdd0/x\ufdd1" failed')).toBe('open path="/x" failed')
  })

  it("drops escaped markers at any depth, keeping the value's own backslashes", () => {
    expect(stripLogMarkers('detail="a \\u{fdd0}x\\u{fdd1} b"')).toBe('detail="a x b"')
    expect(stripLogMarkers('\\\\u{FDD2}x\\\\u{fdd1}')).toBe('x')
    // A value ending in `\` before a depth-1 closer: five backslashes, four of them the value's.
    expect(stripLogMarkers('\\u{fdd0}x\\\\\\\\\\u{fdd1}')).toBe('x\\\\\\\\')
  })

  it('leaves an unmarked line alone', () => {
    const line = 'listing path="docs/Cafe\\u{301}.pdf"'
    expect(stripLogMarkers(line)).toBe(line)
  })
})
