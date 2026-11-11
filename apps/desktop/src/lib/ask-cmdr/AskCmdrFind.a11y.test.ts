/**
 * Tier 3 a11y tests for `AskCmdrFind.svelte`.
 *
 * The rail's in-chat find bar. What this guards: the open bar's search region and field are
 * named, its previous / next / close buttons are named buttons, and the counter and the
 * "loaded messages only" note keep their contrast, with matches, with none, and on a thread
 * whose older pages aren't loaded.
 */

import { afterEach, beforeEach, describe, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

// The registry's ⌘ means Command on a Mac; pin the platform before the registry loads.
vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))

import AskCmdrFind from './AskCmdrFind.svelte'
import { FIND_TYPING_DEBOUNCE_MS } from './ask-cmdr-find.svelte'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  vi.useFakeTimers()
  document.body.innerHTML = ''
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  vi.useRealTimers()
})

/** Mounts the bar over a small thread, opens it with ⌘F, and types `query`. Returns the rail. */
async function openWith(query: string, partial: boolean): Promise<HTMLElement> {
  const rail = document.createElement('aside')
  rail.tabIndex = -1
  const barSlot = document.createElement('div')
  const thread = document.createElement('div')
  // Test fixture markup, written here; no message text reaches it.
  thread.innerHTML = '<p>Rename the report to 2026 tax report</p><p>The report is on the NAS</p>'
  const composer = document.createElement('textarea')
  composer.setAttribute('aria-label', 'Message')
  rail.append(barSlot, thread, composer)
  document.body.appendChild(rail)
  component = mount(AskCmdrFind, { target: barSlot, props: { rail, thread, partial } })
  flushSync()

  composer.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true, bubbles: true, cancelable: true }))
  flushSync()
  await tick()

  const input = rail.querySelector<HTMLInputElement>('[role="search"] input')
  if (!input) throw new Error('find field missing')
  input.value = query
  input.dispatchEvent(new Event('input', { bubbles: true }))
  vi.advanceTimersByTime(FIND_TYPING_DEBOUNCE_MS + 1)
  flushSync()
  await tick()
  return rail
}

describe('AskCmdrFind a11y', () => {
  for (const query of ['report', 'nothing-matches-this']) {
    for (const partial of [false, true]) {
      it(`"${query}" (partial: ${String(partial)}) has no violations`, async () => {
        const rail = await openWith(query, partial)
        // axe schedules its own work, so it runs on real timers once the find has settled.
        vi.useRealTimers()
        await expectNoA11yViolations(rail)
      })
    }
  }
})
