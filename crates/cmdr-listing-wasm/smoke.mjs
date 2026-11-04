// Loads the built module the way a page does and drives one listing through it, so a
// runtime trap only the browser build has (a `std` clock, a thread spawn) fails a check
// instead of a visitor's tab. Run by `scripts/build-listing-wasm.sh -smoke`.
//
// Usage: node smoke.mjs <pkg-dir>
import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

import { smokeWrites, writesTree } from './smoke-writes.mjs'

const pkg = resolve(process.argv[2] ?? 'pkg')
const glue = await import(pathToFileURL(join(pkg, 'cmdr_listing_wasm.js')).href)
await glue.default({ module_or_path: await readFile(join(pkg, 'cmdr_listing_wasm_bg.wasm')) })
const { DemoListing } = glue

function fail(message) {
  console.error(`listing-wasm smoke: ${message}`)
  process.exit(1)
}
// A trap or a dropped promise in the engine must fail the run, not pass it quietly.
process.on('unhandledRejection', (reason) => fail(`unhandled rejection: ${reason?.stack ?? reason}`))

const tree = {
  volumeId: 'root',
  root: '/',
  entries: [
    { path: '/Users', kind: 'dir' },
    { path: '/Users/you', kind: 'dir' },
    { path: '/Users/you/Music', kind: 'dir' },
    { path: '/Users/you/notes 10.txt', kind: 'file', size: 30, modifiedMs: 1_700_000_000_000 },
    { path: '/Users/you/notes 9.txt', kind: 'file', size: 20 },
    { path: '/Users/you/.zshrc', kind: 'file', size: 12 },
    ...writesTree(),
  ],
}

const events = []
const recent = []
const waiters = new Set()
let onEvent = () => {}
const demo = new DemoListing(tree, (name, payload) => {
  events.push(name)
  recent.push({ name, payload })
  for (const waiter of waiters) waiter(name, payload)
  onEvent(name, payload)
})

/** Resolves with the first `name` event (already seen since the last reset, or next) that `match`es. */
function waitFor(name, match, ms) {
  const seen = recent.find((e) => e.name === name && match(e.payload))
  if (seen) return Promise.resolve(seen.payload)
  return new Promise((done, failed) => {
    const waiter = (n, payload) => {
      if (n !== name || !match(payload)) return
      waiters.delete(waiter)
      clearTimeout(timer)
      done(payload)
    }
    const timer = setTimeout(() => {
      waiters.delete(waiter)
      failed(new Error(`no ${name} after ${ms} ms`))
    }, ms)
    waiters.add(waiter)
  })
}

const completed = new Promise((done, failed) => {
  const timer = setTimeout(() => failed(new Error(`no listing-complete after 5 s: ${events.join(', ')}`)), 5000)
  onEvent = (name, payload) => {
    if (name === 'listing-complete') {
      clearTimeout(timer)
      done(payload)
    } else if (name === 'listing-error') {
      clearTimeout(timer)
      failed(new Error(`listing-error: ${JSON.stringify(payload)}`))
    }
  }
})

try {
  const started = await demo.invoke('list_directory_start_streaming', {
    volumeId: 'root',
    path: '/Users/you',
    includeHidden: false,
    sortBy: 'name',
    sortOrder: 'ascending',
    listingId: 'smoke-1',
  })
  if (started.status?.status !== 'loading') fail(`unexpected start answer ${JSON.stringify(started)}`)
  const done = await completed
  if (done.totalCount !== 3) fail(`expected three visible rows, got ${JSON.stringify(done)}`)

  const rows = await demo.invoke('get_file_range', { listingId: 'smoke-1', start: 0, count: 10, includeHidden: false })
  const names = rows.map((row) => row.name).join(' | ')
  if (names !== 'Music | notes 9.txt | notes 10.txt') fail(`rows came back as ${names}`)

  const fuzzy = await demo.invoke('find_first_fuzzy_match', {
    listingId: 'smoke-1',
    query: 'n10',
    includeHidden: false,
  })
  if (fuzzy !== 2) fail(`fuzzy jump answered ${fuzzy}`)

  // The re-read goes through the engine's stall watch, which reads the clock and races a timer.
  const refreshed = await demo.invoke('refresh_listing', { listingId: 'smoke-1', force: true })
  if (refreshed.timedOut !== false) fail(`refresh answered ${JSON.stringify(refreshed)}`)

  // Brief mode's widths, from metrics the page measured. The computation reads the clock,
  // so this is what catches a `std` `Instant` sneaking into it.
  const fontId = 'system-400-12'
  const codePoints = Array.from({ length: 0x7f - 0x20 }, (_, i) => 0x20 + i)
  await demo.invoke('store_font_metrics', { fontId, codePoints, widths: codePoints.map(() => 2) })
  if ((await demo.invoke('has_font_metrics', { fontId })) !== true) fail('stored metrics went missing')
  const brief = await demo.invoke('get_brief_column_text_widths', {
    listingId: 'smoke-1',
    itemsPerColumn: 2,
    hasParent: false,
    fontId,
    includeHidden: false,
  })
  // Columns of two: [Music, notes 9.txt] [notes 10.txt], 2 px per character.
  if (JSON.stringify(brief.widths) !== '[22,24]' || brief.approximate !== false) {
    fail(`Brief widths came back as ${JSON.stringify(brief)}`)
  }

  try {
    await demo.invoke('get_file_range', { listingId: 'gone', start: 0, count: 1, includeHidden: false })
    fail('a missing listing resolved')
  } catch (error) {
    if (error?.type !== 'gone') fail(`a missing listing rejected with ${JSON.stringify(error)}`)
  }

  // The menu bar: a push lands, the bar draws it, and a click names its event.
  const menu = new glue.DemoMenuBar()
  menu.invoke('update_menu_accelerator', { commandId: 'tab.new', shortcut: '⌃T' })
  const tab = menu.bar().find((m) => m.id === 'menu_tab')
  const newTab = tab?.rows.find((row) => row.id === 'new_tab')
  if (newTab?.shortcut !== '⌃T') fail(`the Tab menu's New tab came back as ${JSON.stringify(newTab)}`)
  const click = menu.click('go_home')
  if (click?.kind !== 'emit' || click.payload?.commandId !== 'nav.goHome') {
    fail(`a Go > Home click came back as ${JSON.stringify(click)}`)
  }
  // The runtime seam's browser half: task sets, aborts, macrotask yields, timers, the wall clock.
  const rtSummary = await glue.rtSelfTest(Date.now())
  console.log(`listing-wasm smoke: ${rtSummary}`)

  console.log(`listing-wasm smoke: ${await smokeWrites(demo, { events: recent, waitFor })}`)
} catch (error) {
  fail(error?.stack ?? String(error))
}

console.log(`listing-wasm smoke: OK (${events.length} events, ${DemoListing.commands().length} commands)`)
// The engine's six-hour orphan reaper holds a timer, which would keep Node alive.
process.exit(0)
