/* eslint-disable custom/no-isolated-tests -- it reads the setup file's SOURCE: its subject is the import graph, which importing a module would only add to */
/**
 * Guards the setup file's import closure. Vitest's `related` mode selects EVERY spec when a
 * changed file is reachable from a `setupFiles` entry, so each module `test-setup.ts` reaches
 * turns an edit to it into a full-suite run of `pnpm check svelte-tests-related`. When the
 * closure reached `ipc/bindings.ts` and all of `tauri-commands/` (through the logger and the
 * settings store), three related runs in four were the whole suite:
 * `docs/notes/performance/local-check-lanes-2026-11-05.md`.
 *
 * The walk follows static imports, side-effect imports, and dynamic `import()`s (Vite's graph
 * sees all three), and skips `import type` (erased before Vite sees it) and packages.
 */
import { existsSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const srcDir = path.resolve(import.meta.dirname)
const setupFile = path.join(srcDir, 'test-setup.ts')

const importRe =
  /(?:^|[\s;])(?:import|export)\s+(?!type\s)(?:[^'"]*?\sfrom\s+)?['"]([^'"]+)['"]|import\(\s*['"]([^'"]+)['"]\s*\)/g

function resolveSpecifier(specifier: string, fromFile: string): string | null {
  let base: string
  if (specifier.startsWith('#lib/')) base = path.join(srcDir, 'lib', specifier.slice('#lib/'.length))
  else if (specifier.startsWith('.')) base = path.resolve(path.dirname(fromFile), specifier)
  else return null // a package or a `virtual:` module
  const stem = base.replace(/\.js$/, '')
  for (const candidate of [base, `${stem}.ts`, `${stem}.svelte`, path.join(base, 'index.ts')]) {
    if (existsSync(candidate) && candidate.match(/\.(ts|svelte)$/)) return candidate
  }
  return null
}

function setupClosure(): string[] {
  const seen = new Set<string>([setupFile])
  const queue = [setupFile]
  while (queue.length > 0) {
    const file = queue.pop() as string
    const source = readFileSync(file, 'utf8')
    for (const match of source.matchAll(importRe)) {
      // One of the two capture groups matched: a static import's, or a dynamic one's.
      const resolved = resolveSpecifier(match[1] || match[2], file)
      if (resolved && !seen.has(resolved)) {
        seen.add(resolved)
        queue.push(resolved)
      }
    }
  }
  return [...seen].map((file) => path.relative(srcDir, file)).sort()
}

describe('test-setup.ts import closure', () => {
  const closure = setupClosure()

  it('stays out of the hubs every IPC change regenerates', () => {
    const hubs = closure.filter((file) =>
      ['lib/ipc/', 'lib/tauri-commands/', 'lib/settings/', 'lib/logging/'].some((dir) => file.startsWith(dir)),
    )
    expect(hubs).toEqual([])
  })

  it('stays small, so an edit to almost any module selects only its own tests', () => {
    // Today's closure: the setup file, the message runtime and its locale helpers, and the
    // layout shim. Raise this only for a leaf module that rarely changes.
    expect(closure.length, closure.join('\n')).toBeLessThanOrEqual(12)
  })
})
