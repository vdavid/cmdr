// Loads the built module the way a page does and drives one listing through it, so a
// runtime trap only the browser build has (a `std` clock, a thread spawn) fails a check
// instead of a visitor's tab. Run by `scripts/build-listing-wasm.sh -smoke`.
//
// Usage: node smoke.mjs <pkg-dir>
import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const pkg = resolve(process.argv[2] ?? 'pkg')
const glue = await import(pathToFileURL(join(pkg, 'cmdr_listing_wasm.js')).href)
await glue.default({ module_or_path: await readFile(join(pkg, 'cmdr_listing_wasm_bg.wasm')) })
const { DemoListing } = glue

function fail(message) {
  console.error(`listing-wasm smoke: ${message}`)
  process.exit(1)
}

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
  ],
}

const events = []
let onEvent = () => {}
const demo = new DemoListing(tree, (name, payload) => {
  events.push(name)
  onEvent(name, payload)
})

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

  try {
    await demo.invoke('get_file_range', { listingId: 'gone', start: 0, count: 1, includeHidden: false })
    fail('a missing listing resolved')
  } catch (error) {
    if (error?.type !== 'gone') fail(`a missing listing rejected with ${JSON.stringify(error)}`)
  }
} catch (error) {
  fail(error?.stack ?? String(error))
}

console.log(`listing-wasm smoke: OK (${events.length} events, ${DemoListing.commands().length} commands)`)
// The engine's six-hour orphan reaper holds a timer, which would keep Node alive.
process.exit(0)
