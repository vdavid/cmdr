// The Multi-Rename half of the smoke: ⌃M through `cmdr-multi-rename` in the built module, on
// the page's one volume and write engine. A sheet opens, previews a counter mask, moves a row,
// renames through the journal, and rolls back; a sheet over 1,200 photos reads their media
// fields to `done` (the browser reader answers `NotSupported`, so every photo is unreadable)
// without holding the page past `MAX_GAP_MS` of CPU, in a throttled handful of progress
// events. Called from `smoke.mjs`.

import { holdProbe, MAX_GAP_MS, quiet } from './smoke-writes.mjs'

/** The photos one sheet opens over: the demo's own count. */
const PHOTOS = 1_200

export function multiRenameTree() {
  const photos = Array.from({ length: PHOTOS }, (_, i) => ({
    path: `/Users/r/photos/IMG_${String(i).padStart(4, '0')}.jpg`,
    kind: 'file',
    size: 3_000,
    modifiedMs: 1_700_000_000_000,
  }))
  return [
    { path: '/Users/r', kind: 'dir' },
    { path: '/Users/r/small', kind: 'dir' },
    { path: '/Users/r/small/a.txt', kind: 'file', size: 1 },
    { path: '/Users/r/small/b.txt', kind: 'file', size: 2 },
    { path: '/Users/r/small/c.txt', kind: 'file', size: 3 },
    { path: '/Users/r/photos', kind: 'dir' },
    ...photos,
  ]
}

const spec = (nameMask) => ({
  nameMask,
  extensionMask: '[E]',
  search: '',
  replace: '',
  caseSensitive: false,
  firstOnly: false,
  includeExtension: false,
  regex: false,
  substitute: false,
  case: 'unchanged',
  removeDiacritics: false,
})

/** Drives the sheet; `bus` is `{ events, waitFor(name, match, ms) }`. Throws on the first wrong answer. */
export async function smokeMultiRename(demo, bus) {
  const { waitFor } = bus
  // Key order is the engine's (serde sorts object keys), so compare with sorted keys.
  const canonical = (value) =>
    JSON.stringify(value, (_, v) =>
      v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort()) : v,
    )
  const expect = (what, actual, expected) => {
    if (canonical(actual) !== canonical(expected)) {
      throw new Error(`${what}: ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`)
    }
  }
  const list = async (listingId, path) => {
    await demo.invoke('list_directory_start_streaming', {
      volumeId: 'root',
      path,
      includeHidden: false,
      sortBy: 'name',
      sortOrder: 'ascending',
      listingId,
    })
    await waitFor('listing-complete', (p) => p.listingId === listingId, 5_000)
  }
  const open = (listingId) =>
    demo.invoke('open_multi_rename', { listingId, includeHidden: false, selectedIndices: null, expectedSequence: 0 })
  const names = async (listingId) =>
    (await demo.invoke('get_file_range', { listingId, start: 0, count: 10, includeHidden: false })).map((r) => r.name)
  const newNames = (preview) => preview.rows.map((r) => r.newName)

  // Open, preview, move a row, rename, roll back.
  await list('r-small', '/Users/r/small')
  const sheet = await open('r-small')
  expect('the opened sheet', [sheet.count, sheet.resultsAvailable], [3, false])
  const sessionId = sheet.sessionId
  const first = await demo.invoke('preview_multi_rename', { sessionId, spec: spec('[N]_[C:2]') })
  expect('the counter preview', newNames(first), ['a_01.txt', 'b_02.txt', 'c_03.txt'])
  expect('the moved row', await demo.invoke('move_multi_rename_row', { sessionId, row: 2, direction: 'up' }), 1)
  const moved = await demo.invoke('preview_multi_rename', { sessionId, spec: spec('[N]_[C:2]') })
  expect('the preview after the move', newNames(moved), ['a_01.txt', 'c_02.txt', 'b_03.txt'])
  const started = await demo.invoke('apply_multi_rename', { sessionId, previewId: moved.previewId })
  expect('the rename', started.renaming, 3)
  await waitFor('write-settled', (p) => p.operationId === started.operationId, 10_000)
  await waitFor('directory-diff', (p) => p.listingId === 'r-small', 5_000)
  expect('the renamed pane', await names('r-small'), ['a_01.txt', 'b_03.txt', 'c_02.txt'])
  const history = await demo.invoke('get_multi_rename_history', {})
  expect('the newest history entry', history[0]?.value, '[N]_[C:2]')
  const undone = await demo.invoke('undo_operations', { operationIds: [started.operationId] })
  expect('the roll back', undone.restored, 3)
  expect('the pane after the roll back', await names('r-small'), ['a.txt', 'b.txt', 'c.txt'])
  try {
    await demo.invoke('write_multi_rename_names', { sessionId, previewId: moved.previewId })
    throw new Error('Results wrote a file in a page')
  } catch (error) {
    if (error?.type !== 'couldntWriteNames') throw error
  }
  await demo.invoke('close_multi_rename', { sessionId })

  // 1,200 photos: the media read settles every file inline without holding the page.
  await list('r-photos', '/Users/r/photos')
  const eventsBefore = bus.events.length
  const probe = holdProbe()
  const photos = await open('r-photos')
  const done = await waitFor('multi-rename-media-progress', (p) => p.sessionId === photos.sessionId && p.done, 10_000)
  await quiet(bus.events)
  const { longestCpuMs, longestWallMs } = probe.stop()
  expect('the final media progress', [done.read, done.total], [PHOTOS, PHOTOS])
  const progressEvents = bus.events.slice(eventsBefore).filter((e) => e.name === 'multi-rename-media-progress').length
  if (progressEvents > 3) throw new Error(`the media read sent ${progressEvents} progress events, not a throttled few`)
  if (longestCpuMs > MAX_GAP_MS) {
    throw new Error(
      `the media read held the page for ${Math.round(longestCpuMs)} ms of CPU (${Math.round(longestWallMs)} ms wall)`,
    )
  }
  expect('a late progress ask', await demo.invoke('get_multi_rename_media_progress', { sessionId: photos.sessionId }), {
    read: PHOTOS,
    total: PHOTOS,
    done: true,
  })
  const media = await demo.invoke('preview_multi_rename', { sessionId: photos.sessionId, spec: spec('[=exif.taken]') })
  expect(
    'the media preview',
    [media.counts.problems, media.rows[0].status],
    [PHOTOS, { type: 'unreadable', field: 'exifTaken' }],
  )
  await demo.invoke('close_multi_rename', { sessionId: photos.sessionId })

  const hold = `longest hold ${Math.round(longestCpuMs)} ms CPU, ${Math.round(longestWallMs)} ms wall`
  return `multi-rename OK (a rename and its roll back, ${PHOTOS} photos read in ${progressEvents} progress events, ${hold})`
}
