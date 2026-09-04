/**
 * Runs the REAL boot guard out of `src/app.html`.
 *
 * The guard is the one piece of Cmdr that has to work on a WebKit nobody on the
 * team owns, and nothing else exercises it: it lives in an HTML template, outside
 * the bundle, outside eslint, outside stylelint. So this file extracts the script
 * verbatim, checks it stays ES5, and runs it against a stubbed-out environment
 * with one capability removed at a time.
 *
 * Globals reach the guard as FUNCTION PARAMETERS rather than through
 * `globalThis`, so a test can take `Object.hasOwn` away from the guard without
 * taking it away from vitest.
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { BOOT_GUARD_MARKER } from './gen-boot-guard-lib.ts'
import { WEBKIT_FLOOR_CAPABILITIES } from '../src/lib/utils/webkit-compat.ts'

const SHELL = join(import.meta.dirname, '..', 'src', 'app.html')

/** The guard's JavaScript, exactly as it ships. */
function guardSource(): string {
  const html = readFileSync(SHELL, 'utf8')
  const match = /<script>([\s\S]*?)<\/script>/.exec(html)
  if (!match) throw new Error('`src/app.html` no longer carries the boot-guard <script>')
  return match[1]
}

/** The guard with its comments removed, for the syntax sweep. */
function guardCode(): string {
  return guardSource()
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/\/\/[^\n]*/g, ' ')
}

const PAYLOAD = {
  strings: {
    en: { title: 'Cmdr needs a newer Safari', body: 'Install your updates.', quit: 'Quit' },
    hu: { title: 'A Cmdrnek újabb Safari kell', body: 'Telepítsd a frissítéseket.', quit: 'Kilépés' },
    'zh-Hant': { title: '需要較新的 Safari', body: '請更新。', quit: '結束' },
  },
  aliases: { en: 'en', hu: 'hu', 'zh-hant': 'zh-Hant', 'zh-tw': 'zh-Hant' },
  force: false,
}

interface Capabilities {
  randomUUID?: boolean
  hasOwn?: boolean
  findLast?: boolean
  hasSelector?: boolean
}

interface RunOptions {
  capabilities?: Capabilities
  languages?: string[]
  force?: boolean
  invoke?: (cmd: string, args: unknown) => Promise<unknown>
}

/**
 * Executes the guard with the given environment and returns what it did.
 * @param options.capabilities which Safari 15.4 features the fake WebKit has (all, by default)
 * @param options.languages what `navigator.languages` reports
 * @param options.force bake in the dev override
 * @param options.invoke the Tauri IPC stand-in, if the test cares about Quit
 */
function runGuard(options: RunOptions = {}): { blocked: boolean; lang: string; title: string; quit: string } {
  const capabilities = { randomUUID: true, hasOwn: true, findLast: true, hasSelector: true, ...options.capabilities }

  const fakeObject = Object.create(Object) as ObjectConstructor
  if (!capabilities.hasOwn) Object.defineProperty(fakeObject, 'hasOwn', { value: undefined })

  const fakeArrayPrototype = Object.create(Array.prototype) as unknown[]
  if (!capabilities.findLast) Object.defineProperty(fakeArrayPrototype, 'findLast', { value: undefined })
  const fakeArray = Object.create(Array) as ArrayConstructor
  Object.defineProperty(fakeArray, 'prototype', { value: fakeArrayPrototype })

  const fakeCrypto = capabilities.randomUUID ? { randomUUID: () => 'x' } : {}
  const fakeCss = { supports: (query: string) => (query.includes('selector(') ? capabilities.hasSelector : true) }
  const fakeWindow = {
    __TAURI_INTERNALS__: options.invoke ? { invoke: options.invoke } : undefined,
  }
  const fakeNavigator = { languages: options.languages ?? ['en-US'], language: (options.languages ?? ['en-US'])[0] }

  const source = guardSource().replace(BOOT_GUARD_MARKER, JSON.stringify({ ...PAYLOAD, force: options.force === true }))
  // The guard IS a string of ES5 that ships inside an HTML file, so evaluating it
  // is the only way to test what actually runs. Parameters shadow the globals, so
  // a fake WebKit stays inside this call.
  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  const run = new Function('crypto', 'CSS', 'Object', 'Array', 'navigator', 'window', 'document', source) as (
    ...args: unknown[]
  ) => void
  run(fakeCrypto, fakeCss, fakeObject, fakeArray, fakeNavigator, fakeWindow, document)

  const block = document.querySelector('.cmdr-boot-block')
  return {
    blocked: block !== null,
    lang: document.documentElement.lang,
    title: block?.querySelector('h1')?.textContent ?? '',
    quit: block?.querySelector('button')?.textContent ?? '',
  }
}

beforeEach(() => {
  document.head.innerHTML = ''
  document.body.innerHTML = '<div id="loading-screen"></div><div id="app-root"></div>'
  document.documentElement.lang = 'en'
})

describe('syntax', () => {
  // ❗ Every one of these parses fine in Node and in vitest, and NONE of them
  // parses on the Safari the guard exists for. The guard would die silently and
  // the user would be back to a blank window.
  const banned: [RegExp, string][] = [
    [/(^|[^\w.])(const|let)\s/, '`const` / `let` (Safari 10 in strict mode, but this file is the belt AND braces)'],
    [/=>/, 'arrow functions'],
    [/`/, 'template literals'],
    [/\?\./, 'optional chaining (Safari 13.1)'],
    [/\?\?/, 'nullish coalescing (Safari 13.1)'],
    [/(\|\||&&|\?\?)=/, 'logical assignment (Safari 14), the exact syntax that makes the bundle unparseable'],
    [/(^|[^\w.])class\s+\w/, 'class syntax'],
    [/\.\.\./, 'spread / rest'],
    [/(^|[^\w.])async\s/, '`async` functions'],
    [/(^|[^\w.])for\s*\(\s*\w+\s+of\s/, '`for … of`'],
  ]

  for (const [pattern, what] of banned) {
    it(`carries no ${what}`, () => {
      expect(guardCode()).not.toMatch(pattern)
    })
  }

  it('probes every capability `WEBKIT_FLOOR_CAPABILITIES` names', () => {
    // The guard can't import the list (it runs before any module), so this is
    // what keeps the two sides from drifting apart.
    const probes: Record<string, string> = {
      'crypto.randomUUID': 'crypto.randomUUID',
      'Object.hasOwn': 'Object.hasOwn',
      'Array.prototype.findLast': 'Array.prototype.findLast',
      ':has()': ':has(*)',
    }
    for (const capability of WEBKIT_FLOOR_CAPABILITIES) {
      expect(guardCode(), capability).toContain(probes[capability])
    }
  })
})

describe('on a WebKit that meets the floor', () => {
  it('stays out of the way entirely', () => {
    const result = runGuard()
    expect(result.blocked).toBe(false)
    expect(document.querySelector('#app-root')).not.toBeNull()
  })

  it('blocks anyway under the dev override', () => {
    expect(runGuard({ force: true }).blocked).toBe(true)
  })
})

describe('below the floor', () => {
  const missing: [string, Capabilities][] = [
    ['crypto.randomUUID', { randomUUID: false }],
    ['Object.hasOwn', { hasOwn: false }],
    ['Array.prototype.findLast', { findLast: false }],
    [':has()', { hasSelector: false }],
  ]

  for (const [capability, capabilities] of missing) {
    it(`blocks when ${capability} is missing`, () => {
      expect(runGuard({ capabilities }).blocked).toBe(true)
    })
  }

  it('takes the page away from SvelteKit, so a half-parsed bundle can never paint over it', () => {
    runGuard({ capabilities: { hasOwn: false } })
    expect(document.querySelector('#app-root')).toBeNull()
    expect(document.querySelector('#loading-screen')).toBeNull()
  })

  it("speaks the reader's language and stamps `<html lang>`", () => {
    const result = runGuard({ capabilities: { hasOwn: false }, languages: ['hu-HU', 'en-US'] })
    expect(result.title).toBe(PAYLOAD.strings.hu.title)
    expect(result.quit).toBe(PAYLOAD.strings.hu.quit)
    expect(result.lang).toBe('hu')
  })

  it('sends a Traditional-Chinese reader to the Traditional catalog', () => {
    // Dropping `-TW` and matching `zh` would be text this reader can't read.
    expect(runGuard({ capabilities: { hasOwn: false }, languages: ['zh-TW'] }).title).toBe(
      PAYLOAD.strings['zh-Hant'].title,
    )
  })

  it('falls back to English for a language Cmdr does not ship', () => {
    const result = runGuard({ capabilities: { hasOwn: false }, languages: ['is-IS'] })
    expect(result.title).toBe(PAYLOAD.strings.en.title)
    expect(result.lang).toBe('en')
  })

  it('quits through Tauri, so the quit gate still sees it', () => {
    const calls: string[] = []
    runGuard({
      capabilities: { hasOwn: false },
      invoke: (cmd) => {
        calls.push(cmd)
        return Promise.resolve()
      },
    })
    document.querySelector<HTMLButtonElement>('.cmdr-boot-block button')?.click()
    expect(calls).toEqual(['plugin:process|exit'])
  })

  it('survives a webview with no Tauri IPC, rather than throwing on click', () => {
    runGuard({ capabilities: { hasOwn: false } })
    expect(() => document.querySelector<HTMLButtonElement>('.cmdr-boot-block button')?.click()).not.toThrow()
  })
})
