/**
 * Vite plugin: fails the build when the emitted client bundle holds something the oldest WebKit
 * Cmdr promises (Safari 15.4; a fully patched Catalina gets 15.6.1) can't run or drops, and that
 * `build.target` / `build.cssTarget` can't lower. Three finders over every emitted chunk and
 * stylesheet, dependencies included, with one parse per chunk:
 *
 * - `vite-webkit-regex-floor.ts`: regex syntax (0.59.0's blank window on Catalina).
 * - `vite-webkit-api-floor.ts`: unchecked calls to newer JavaScript and DOM functions.
 * - `vite-webkit-css-floor.ts`: CSS that old WebKit drops without a fallback.
 *
 * What counts as newer comes from MDN's browser-compat-data (`vite-webkit-floor-compat.ts`).
 * Why the floors exist and how they relate to the boot guard:
 * `docs/notes/system-requirements-and-es2025.md` § The two floors old WebKit crosses.
 */

import type { Plugin } from 'vite'
import { describeApiFindings, findApisBeyondSafari15, type ApiFinding } from './vite-webkit-api-floor.ts'
import {
  describeCssFindings,
  findCssBeyondSafari15,
  supportsNotFallbacks,
  type CssFinding,
} from './vite-webkit-css-floor.ts'
import { describeFindings, findRegexInProgram, parseChunk, type RegexFinding } from './vite-webkit-regex-floor.ts'

/** An emitted stylesheet's text. */
function cssText(source: string | Uint8Array): string {
  return typeof source === 'string' ? source : new TextDecoder().decode(source)
}

/** Fails `vite build` on any emitted regex, API call, or CSS beyond the WebKit floor. A no-op in `vite dev`. */
export function webkitFloor(): Plugin {
  return {
    name: 'cmdr-webkit-floor',
    apply: 'build',
    // Only the client bundle reaches WebKit; SvelteKit's server build runs in Node at build time.
    applyToEnvironment: (environment) => environment.name === 'client',
    generateBundle(_options, bundle) {
      const regexByChunk = new Map<string, RegexFinding[]>()
      const apiByChunk = new Map<string, ApiFinding[]>()
      const stylesheets = new Map<string, string>()
      for (const output of Object.values(bundle)) {
        if (output.type === 'asset') {
          if (output.fileName.endsWith('.css')) stylesheets.set(output.fileName, cssText(output.source))
          continue
        }
        const program = parseChunk(output.code)
        const regexes = findRegexInProgram(program)
        if (regexes.length > 0) regexByChunk.set(output.fileName, regexes)
        const apis = findApisBeyondSafari15(program, output.fileName)
        if (apis.length > 0) apiByChunk.set(output.fileName, apis)
      }
      // An `@supports not` fallback in one stylesheet covers the feature in all of them.
      const fallbacks = new Set([...stylesheets.values()].flatMap((css) => [...supportsNotFallbacks(css)]))
      const cssByFile = new Map<string, CssFinding[]>()
      for (const [fileName, css] of stylesheets) {
        const findings = findCssBeyondSafari15(css, fallbacks)
        if (findings.length > 0) cssByFile.set(fileName, findings)
      }

      const sections: string[] = []
      if (regexByChunk.size > 0) sections.push(describeFindings(regexByChunk))
      if (apiByChunk.size > 0) sections.push(describeApiFindings(apiByChunk).join('\n'))
      if (cssByFile.size > 0) sections.push(describeCssFindings(cssByFile).join('\n'))
      if (sections.length > 0) this.error(sections.join('\n\n'))
    },
  }
}
