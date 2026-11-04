/**
 * Vite plugin: fails the build when an emitted client chunk holds something the oldest WebKit
 * Cmdr promises (Safari 15.4; a fully patched Catalina gets 15.6.1) can't run, and that the
 * `build.target` can't lower. Two finders, one parse per chunk, dependencies included:
 *
 * - `vite-webkit-regex-floor.ts`: regex syntax (0.59.0's blank window on Catalina).
 * - `vite-webkit-api-floor.ts`: unchecked calls to newer JavaScript and DOM functions.
 *
 * Why both floors exist and how they relate to the boot guard:
 * `docs/notes/system-requirements-and-es2025.md` § The two floors old WebKit crosses.
 */

import type { Plugin } from 'vite'
import { describeApiFindings, findApisBeyondSafari15, type ApiFinding } from './vite-webkit-api-floor.ts'
import { describeFindings, findRegexInProgram, parseChunk, type RegexFinding } from './vite-webkit-regex-floor.ts'

/** Fails `vite build` on any emitted regex or API call beyond the WebKit floor. A no-op in `vite dev`. */
export function webkitFloor(): Plugin {
  return {
    name: 'cmdr-webkit-floor',
    apply: 'build',
    // Only the client bundle reaches WebKit; SvelteKit's server build runs in Node at build time.
    applyToEnvironment: (environment) => environment.name === 'client',
    generateBundle(_options, bundle) {
      const regexByChunk = new Map<string, RegexFinding[]>()
      const apiByChunk = new Map<string, ApiFinding[]>()
      for (const output of Object.values(bundle)) {
        if (output.type !== 'chunk') continue
        const program = parseChunk(output.code)
        const regexes = findRegexInProgram(program)
        if (regexes.length > 0) regexByChunk.set(output.fileName, regexes)
        const apis = findApisBeyondSafari15(program, output.fileName)
        if (apis.length > 0) apiByChunk.set(output.fileName, apis)
      }
      const sections: string[] = []
      if (regexByChunk.size > 0) sections.push(describeFindings(regexByChunk))
      if (apiByChunk.size > 0) sections.push(describeApiFindings(apiByChunk).join('\n'))
      if (sections.length > 0) this.error(sections.join('\n\n'))
    },
  }
}
