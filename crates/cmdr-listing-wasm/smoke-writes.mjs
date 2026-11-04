// The write half of the smoke: every write command through the real write engine in the
// built module, on the page's one in-memory volume. A copy, a conflict answered Skip, a
// cancel, a move, a delete, a rename, a new folder, an undo, the journal read back, a
// `directory-diff` for the open pane after each, and a long delete that must keep handing
// the page back (no gap between timer ticks past `MAX_GAP_MS` of the main thread's CPU
// time). Called from `smoke.mjs`.

/** The longest the engine may hold the page between two timer ticks, in main-thread CPU time. */
const MAX_GAP_MS = 50
/** The files the long-task probe deletes in one go. */
const MANY = 1_200

export function writesTree() {
  const many = Array.from({ length: MANY }, (_, i) => ({ path: `/Users/w/many/f${i}.txt`, kind: 'file', size: 100 }))
  return [
    { path: '/Users/w', kind: 'dir' },
    { path: '/Users/w/Documents', kind: 'dir' },
    { path: '/Users/w/Documents/b.txt', kind: 'file', size: 7 },
    { path: '/Users/w/a.txt', kind: 'file', size: 1_000 },
    { path: '/Users/w/b.txt', kind: 'file', size: 2_000 },
    { path: '/Users/w/c.txt', kind: 'file', size: 3_000 },
    { path: '/Users/w/big.bin', kind: 'file', size: 64 * 1024 * 1024 },
    { path: '/Users/w/many', kind: 'dir' },
    ...many,
  ]
}

const config = (conflictResolution) => ({ progressIntervalMs: 50, conflictResolution, maxConflictsToShow: 100 })

/** Drives the writes; `bus` is `{ events, waitFor(name, match, ms) }`. Throws on the first wrong answer. */
export async function smokeWrites(demo, bus) {
  const { waitFor } = bus
  const settled = async (operationId) => {
    const of = (p) => p.operationId === operationId
    await waitFor('write-settled', of, 10_000)
    const ended = ['write-complete', 'write-cancelled', 'write-error'].find((name) =>
      bus.events.some((e) => e.name === name && of(e.payload)),
    )
    return ended
  }
  const diffFor = (listingId) => waitFor('directory-diff', (p) => p.listingId === listingId, 5_000)
  const rowsOf = async (listingId) =>
    (await demo.invoke('get_file_range', { listingId, start: 0, count: 50, includeHidden: false })).map((r) => [
      r.name,
      r.size,
    ])
  const open = async (listingId, path) => {
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
  const copy = async (sources, destPath, conflictResolution) =>
    (
      await demo.invoke('copy_between_volumes', {
        sourceVolumeId: 'root',
        sourcePaths: sources,
        destVolumeId: 'root',
        destPath,
        config: config(conflictResolution),
        initiator: 'user',
      })
    ).operationId
  const expect = (what, actual, expected) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${what}: ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`)
    }
  }

  await open('w-docs', '/Users/w/Documents')
  await open('w-home', '/Users/w')

  // Copy two files: each lands with its source's size, and the pane hears it.
  bus.events.length = 0
  const copied = await copy(['/Users/w/a.txt', '/Users/w/c.txt'], '/Users/w/Documents', 'stop')
  expect('the copy', await settled(copied), 'write-complete')
  await diffFor('w-docs')
  expect('the copied rows', await rowsOf('w-docs'), [
    ['a.txt', 1_000],
    ['b.txt', 7],
    ['c.txt', 3_000],
  ])

  // Undo it, through the real planner: both copies go, the originals stay.
  const undone = await demo.invoke('undo_operations', { operationIds: [copied] })
  expect(`the undo (${JSON.stringify(undone)})`, undone.restored, 2)
  expect('the rows after the undo', await rowsOf('w-docs'), [['b.txt', 7]])

  // A conflict asks, and Skip keeps the destination.
  const clash = await copy(['/Users/w/b.txt'], '/Users/w/Documents', 'stop')
  const asked = await waitFor('write-conflict', (p) => p.operationId === clash, 5_000)
  expect(
    'the conflict answer',
    await demo.invoke('resolve_write_conflict', {
      operationId: clash,
      conflictId: asked.conflictId,
      resolution: 'skip',
      applyToAll: false,
    }),
    'resolved',
  )
  expect('the skipped copy', await settled(clash), 'write-complete')

  // A paced copy cancels.
  const big = await copy(['/Users/w/big.bin'], '/Users/w/Documents', 'stop')
  await waitFor('write-progress', (p) => p.operationId === big, 5_000)
  await demo.invoke('cancel_write_operation', { operationId: big, rollback: false })
  expect('the cancelled copy', await settled(big), 'write-cancelled')

  // New folder, rename, move (the desktop's same-root move), delete.
  bus.events.length = 0
  expect(
    'the new folder',
    (
      await demo.invoke('create_directory', {
        volumeId: 'root',
        parentPath: '/Users/w',
        name: 'New',
        initiator: 'user',
      })
    ).type,
    'done',
  )
  await diffFor('w-home')
  expect(
    'the rename',
    (
      await demo.invoke('rename_file', {
        from: '/Users/w/a.txt',
        to: '/Users/w/z.txt',
        force: false,
        volumeId: 'root',
        initiator: 'user',
      })
    ).type,
    'done',
  )
  const moved = await demo.invoke('move_files', {
    sources: ['/Users/w/c.txt'],
    destination: '/Users/w/New',
    config: config('stop'),
    initiator: 'user',
  })
  expect('the move', await settled(moved.operationId), 'write-complete')
  const deleted = await demo.invoke('delete_files', {
    sources: ['/Users/w/b.txt'],
    volumeId: 'root',
    initiator: 'user',
  })
  expect('the delete', await settled(deleted.operationId), 'write-complete')

  // Every operation journaled, and the pane shows them all.
  const log = await demo.invoke('get_recent_operation_log_entries', { limit: 50, offset: 0 })
  const kinds = log.map((row) => row.kind).sort()
  for (const kind of ['copy', 'delete', 'move', 'rename']) {
    if (!kinds.includes(kind)) throw new Error(`no ${kind} in the journal: ${JSON.stringify(kinds)}`)
  }
  const home = (await rowsOf('w-home')).map(([name]) => name)
  expect('the home pane', home, ['Documents', 'many', 'New', 'big.bin', 'z.txt'])

  // A long delete keeps handing the page back.
  const probe = holdProbe()
  const many = await demo.invoke('delete_files', { sources: ['/Users/w/many'], volumeId: 'root', initiator: 'user' })
  expect('the long delete', await settled(many.operationId), 'write-complete')
  const { longestCpuMs, longestWallMs } = probe.stop()
  if (longestCpuMs > MAX_GAP_MS) {
    throw new Error(
      `the delete held the page for ${Math.round(longestCpuMs)} ms of CPU (${Math.round(longestWallMs)} ms wall)`,
    )
  }

  const space = await demo.invoke('get_volume_space', { path: '/' })
  expect('the space answer', space.data.kind, 'bounded')
  const hold = `longest hold ${Math.round(longestCpuMs)} ms CPU, ${Math.round(longestWallMs)} ms wall`
  return `writes OK (journal ${log.length} rows, ${hold})`
}

/**
 * Measures the longest the page goes between two timer ticks, as the main thread's own CPU
 * time (`process.threadCpuUsage`) beside wall time. Only the CPU figure gates: a loaded
 * machine stretches wall time by descheduling the process, but can't add CPU time to the
 * thread, so the gate fails only when the engine itself holds the page.
 */
function holdProbe() {
  const cpuMs = () => {
    const { user, system } = process.threadCpuUsage()
    return (user + system) / 1000
  }
  let longestCpuMs = 0
  let longestWallMs = 0
  let lastCpu = cpuMs()
  let lastWall = performance.now()
  let probing = true
  const tick = () => {
    const cpu = cpuMs()
    const wall = performance.now()
    longestCpuMs = Math.max(longestCpuMs, cpu - lastCpu)
    longestWallMs = Math.max(longestWallMs, wall - lastWall)
    lastCpu = cpu
    lastWall = wall
    if (probing) setTimeout(tick, 0)
  }
  setTimeout(tick, 0)
  return {
    stop() {
      probing = false
      return { longestCpuMs, longestWallMs }
    },
  }
}
