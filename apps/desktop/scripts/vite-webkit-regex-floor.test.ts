import { describe, expect, it } from 'vitest'
import { describeFindings, findRegexBeyondSafari15, regexFeaturesBeyondSafari15 } from './vite-webkit-regex-floor.ts'

describe('regexFeaturesBeyondSafari15', () => {
  it('flags both lookbehind forms', () => {
    expect(regexFeaturesBeyondSafari15('(?<=a)b', '')).toEqual(['lookbehind'])
    expect(regexFeaturesBeyondSafari15('(?<![\\p{L}\\p{N}+.-])s3:', 'gu')).toEqual(['lookbehind'])
  })

  it('flags the v flag, inline modifiers, and a duplicate group name', () => {
    expect(regexFeaturesBeyondSafari15('[\\p{L}--a]', 'v')).toEqual(['unicodeSets'])
    expect(regexFeaturesBeyondSafari15('(?i:a)b', '')).toEqual(['modifiers'])
    expect(regexFeaturesBeyondSafari15('(?<y>\\d{4})-x|x-(?<y>\\d{4})', '')).toEqual(['duplicateNamedGroups'])
  })

  it('passes what Safari 15 compiles: lookahead, named groups, property escapes, the d flag', () => {
    expect(regexFeaturesBeyondSafari15('^\\u0000\\d+(?=\\/|$)', '')).toEqual([])
    expect(regexFeaturesBeyondSafari15('(?!x)(?<name>\\p{L}+)\\k<name>', 'gud')).toEqual([])
    // `(?<name>` and `(?<=` share a prefix; a named group must not read as a lookbehind.
    expect(regexFeaturesBeyondSafari15('(?<a>x)(?<b>y)', '')).toEqual([])
  })
})

describe('findRegexBeyondSafari15', () => {
  it('finds a literal, and the RegExp() call the bundler rewrites an unsupported literal into', () => {
    // The 0.59.0 shape exactly: what the build emitted for `display-path.ts`'s S3 pattern.
    const emitted = 'var a=/(?<=x)y/g,sn=RegExp(`(?<![\\\\p{L}\\\\p{N}+.-])s3:\\\\/\\\\/[A-Za-z0-9]+@`,`gu`);'
    expect(findRegexBeyondSafari15(emitted)).toEqual([
      { pattern: '(?<=x)y', flags: 'g', features: ['lookbehind'] },
      { pattern: '(?<![\\p{L}\\p{N}+.-])s3:\\/\\/[A-Za-z0-9]+@', flags: 'gu', features: ['lookbehind'] },
    ])
  })

  it('checks `new RegExp` with string literals, and skips patterns built at runtime', () => {
    const code = 'new RegExp("(?<=a)", "u"); RegExp(p, "u"); new RegExp(`(?<=${x})`); f(/a/g)'
    expect(findRegexBeyondSafari15(code)).toEqual([{ pattern: '(?<=a)', flags: 'u', features: ['lookbehind'] }])
  })

  it('reads ES modules and leaves a clean chunk clean', () => {
    expect(findRegexBeyondSafari15('import x from "./x.js"; export const r = /(?=a)b/u; export default x')).toEqual([])
  })
})

describe('describeFindings', () => {
  it('names the chunk, the pattern, and the Safari that added the feature', () => {
    const text = describeFindings(
      new Map([['_app/immutable/chunks/A.js', [{ pattern: '(?<=a)', flags: 'u', features: ['lookbehind' as const] }]]]),
    )
    expect(text).toContain('_app/immutable/chunks/A.js: /(?<=a)/u: a lookbehind assertion')
    expect(text).toContain('Safari 16.4')
  })
})
