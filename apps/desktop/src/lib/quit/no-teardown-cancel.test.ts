/**
 * The regression anchor for the defect the quit gate replaced.
 *
 * `(main)/+layout.svelte` used to register a `beforeunload` handler calling
 * `cancelAllWriteOperations()`, which walks the GLOBAL registry: a dev hot-reload
 * killed a backgrounded transfer while the queue window still rendered its row,
 * and on the real quit path it was a `void`-ed call nothing awaited. Stopping
 * work is the quit gate's job now, and a window going away is not an event an
 * operation hears about.
 *
 * **Why a source scan rather than a behavioral test.** The behavior can't be
 * pinned honestly at any other level. A Vitest mount of the layout proves only
 * that one file's listeners, and the invariant is about the whole frontend. An
 * E2E reload can't produce a red either: the old handler's IPC raced page
 * teardown, so the operation survived a reload roughly half the time even with
 * the defect in place (measured, 2026-08-10) — a test that goes green on a
 * broken build is worse than none. What IS deterministic is that no frontend
 * code reaches for the command at all, so that's what this asserts.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
// The module that used to expose the footgun; imported so the first assertion
// below is about real source, not a string.
import * as writeOperationsIpc from '$lib/tauri-commands/write-operations'

/** Vitest's root is `apps/desktop` (see `vitest.config.ts`). */
const SRC = resolve(process.cwd(), 'src') + '/'

/** Where the command legitimately appears: the generated bindings, and nothing else. */
const ALLOWED = ['lib/ipc/bindings.ts']

function sourceFiles(dir: string, found: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      sourceFiles(full, found)
    } else if (/\.(ts|svelte)$/.test(entry)) {
      found.push(full)
    }
  }
  return found
}

describe('a window going away never stops work', () => {
  it('the IPC layer exposes no wrapper for cancelling every operation', () => {
    expect(Object.keys(writeOperationsIpc)).not.toContain('cancelAllWriteOperations')
  })

  it('no frontend code cancels the whole write-operation registry', () => {
    const offenders = sourceFiles(SRC)
      .filter((file) => !ALLOWED.some((allowed) => file.endsWith(allowed)))
      .filter((file) => !file.endsWith('no-teardown-cancel.test.ts'))
      .filter((file) => readFileSync(file, 'utf8').includes('cancelAllWriteOperations'))
      .map((file) => file.slice(SRC.length))

    expect(
      offenders,
      'Stopping every operation at once belongs to the quit gate (src-tauri/src/quit/). ' +
        'If a window needs to clean up after itself, clean up the WINDOW.',
    ).toEqual([])
  })

  it('the main layout registers no unload handler at all', () => {
    const layout = readFileSync(join(SRC, 'routes/(main)/+layout.svelte'), 'utf8')
    expect(layout).not.toContain('beforeunload')
    expect(layout).not.toContain('unload')
  })
})
