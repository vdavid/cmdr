import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createOsDrops,
  importDropped,
  INCOMING_PATH,
  readDrop,
  type DropImporter,
  type ImportItem,
  type ImportPlan,
  type ImportRefusal,
  type OsDrops,
} from './os-drop'

function file(name: string, text: string, lastModified = 1_700_000_000_000): File {
  return new File([text], name, { lastModified })
}

/** A file entry, as `webkitGetAsEntry` hands one over. */
function fileEntry(f: File): FileSystemFileEntry {
  return {
    name: f.name,
    isFile: true,
    isDirectory: false,
    file: (resolve: (f: File) => void) => {
      resolve(f)
    },
  } as unknown as FileSystemFileEntry
}

/** A folder entry whose reader hands its children over in batches of `batchSize`, then an empty one. */
function folderEntry(name: string, children: FileSystemEntry[], batchSize = 1): FileSystemDirectoryEntry {
  return {
    name,
    isFile: false,
    isDirectory: true,
    createReader: () => {
      let at = 0
      return {
        readEntries: (resolve: (batch: FileSystemEntry[]) => void) => {
          const batch = children.slice(at, at + batchSize)
          at += batch.length
          resolve(batch)
        },
      }
    },
  } as unknown as FileSystemDirectoryEntry
}

function item(entry: FileSystemEntry | null, f: File | null = null) {
  return { kind: 'file', getAsFile: () => f, webkitGetAsEntry: () => entry }
}

const strip = (items: ImportItem[]) => items.map(({ path, kind, size }) => ({ path, kind, size }))

describe('readDrop', () => {
  it('keeps a folder’s structure, folders before what they hold', async () => {
    const a = file('a.txt', 'hello')
    const b = file('b.txt', 'hi')
    const notes = file('notes.md', '# x')
    const trip = folderEntry('trip', [fileEntry(b), folderEntry('day 1', [fileEntry(a)])])
    const dropped = await readDrop({ items: [item(trip), item(fileEntry(notes))], files: [] })
    expect(strip(dropped)).toEqual([
      { path: 'trip', kind: 'folder', size: 0 },
      { path: 'trip/b.txt', kind: 'file', size: 2 },
      { path: 'trip/day 1', kind: 'folder', size: 0 },
      { path: 'trip/day 1/a.txt', kind: 'file', size: 5 },
      { path: 'notes.md', kind: 'file', size: 3 },
    ])
    expect(dropped[1].file).toBe(b)
    expect(dropped[4].modifiedMs).toBe(1_700_000_000_000)
  })

  it('falls back to the flat files where the browser has no entries', async () => {
    const a = file('a.txt', 'hello')
    const dropped = await readDrop({ items: [{ kind: 'file', getAsFile: () => a }], files: [a] })
    expect(strip(dropped)).toEqual([{ path: 'a.txt', kind: 'file', size: 5 }])
    const noItems = await readDrop({ files: [a] })
    expect(strip(noItems)).toEqual([{ path: 'a.txt', kind: 'file', size: 5 }])
  })
})

function fakeImporter(plan: Partial<ImportPlan> = {}, refuseIndex?: number) {
  const landed: { batch: number; index: number; text: string }[] = []
  const handedOff: { batch: number; offered: boolean }[] = []
  const planned: ImportItem[][] = []
  const importer: DropImporter = {
    plan: (items) => {
      planned.push([...items])
      const roots = items.filter((i) => !i.path.includes('/')).map((i) => `/.drops/1/${i.path}`)
      return Promise.resolve({ batch: 1, accepted: items.length, roots, refusal: null, ...plan })
    },
    file: (batch, index, bytes) => {
      if (index === refuseIndex) return Promise.reject(new Error('sizeChanged'))
      landed.push({ batch, index, text: new TextDecoder().decode(bytes) })
      return Promise.resolve(null)
    },
    handOff: (batch, offered) => {
      handedOff.push({ batch, offered })
      return Promise.resolve(null)
    },
    releaseOffered: () => Promise.resolve(null),
  }
  return { importer, landed, planned, handedOff }
}

describe('importDropped', () => {
  it('plans without bytes, then lands each accepted file’s bytes', async () => {
    const { importer, landed, planned } = fakeImporter()
    const dropped = await readDrop({
      items: [item(folderEntry('trip', [fileEntry(file('a.txt', 'hello'))])), item(fileEntry(file('b.txt', 'hi')))],
      files: [],
    })
    const result = await importDropped(importer, dropped)
    expect(planned[0]).not.toHaveProperty('0.file')
    expect(landed).toEqual([
      { batch: 1, index: 1, text: 'hello' },
      { batch: 1, index: 2, text: 'hi' },
    ])
    expect(result).toEqual({ batch: 1, paths: ['/.drops/1/trip', '/.drops/1/b.txt'], refusal: null })
  })

  it('reads no bytes past what the budget accepted, and passes its refusal on', async () => {
    const refusal: ImportRefusal = { kind: 'overBudget', budgetBytes: 10_000_000, leftOut: 1 }
    const { importer, landed } = fakeImporter({ accepted: 1, roots: ['/.drops/1/a.txt'], refusal })
    const dropped = await readDrop({ files: [file('a.txt', 'a'), file('b.txt', 'b')] })
    const result = await importDropped(importer, dropped)
    expect(landed.map((l) => l.index)).toEqual([0])
    expect(result).toEqual({ batch: 1, paths: ['/.drops/1/a.txt'], refusal })
  })

  it('leaves a top-level file that didn’t land out of the drop', async () => {
    const { importer } = fakeImporter({}, 0)
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    const dropped = await readDrop({ files: [file('a.txt', 'a'), file('b.txt', 'b')] })
    expect((await importDropped(importer, dropped)).paths).toEqual(['/.drops/1/b.txt'])
  })
})

/** A browser drag event carrying files (or not), at a point, with keys. */
function drag(
  type: string,
  init: { x?: number; y?: number; files?: File[]; types?: string[]; altKey?: boolean } = {},
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true })
  const files = init.files ?? []
  const dataTransfer = {
    types: init.types ?? ['Files'],
    files,
    items: files.map((f) => ({ kind: 'file', getAsFile: () => f })),
  }
  Object.assign(event, {
    clientX: init.x ?? 10,
    clientY: init.y ?? 20,
    altKey: init.altKey ?? false,
    metaKey: false,
    shiftKey: false,
  })
  Object.defineProperty(event, 'dataTransfer', { value: dataTransfer })
  return event
}

describe('createOsDrops', () => {
  let sent: { event: string; payload: unknown }[]
  let refusals: ImportRefusal[]
  let drops: OsDrops
  let fake: ReturnType<typeof fakeImporter>

  function setUp(plan: Partial<ImportPlan> = {}): void {
    fake = fakeImporter(plan)
    drops = createOsDrops(
      window,
      (event, payload) => sent.push({ event, payload }),
      fake.importer,
      (refusal) => refusals.push(refusal),
    )
  }

  beforeEach(() => {
    sent = []
    refusals = []
  })
  afterEach(() => {
    drops.dispose()
  })

  const names = () => sent.map((s) => s.event)
  const settled = () => vi.waitFor(() => expect(names()).toContain('tauri://drag-drop'))

  it('sends wry’s events for a drag over the page, then the drop with the staged paths', async () => {
    setUp()
    window.dispatchEvent(drag('dragenter', { x: 5, y: 6 }))
    document.body.dispatchEvent(drag('dragenter'))
    window.dispatchEvent(drag('dragleave'))
    const over = drag('dragover', { x: 7, y: 8 })
    window.dispatchEvent(over)
    expect(over.defaultPrevented).toBe(true)
    window.dispatchEvent(drag('drop', { x: 9, y: 10, files: [file('a.txt', 'hello')] }))
    await settled()

    expect(names()).toEqual([
      'drag-image-size',
      'drag-modifiers',
      'tauri://drag-enter',
      'tauri://drag-over',
      'tauri://drag-drop',
    ])
    expect(sent[2].payload).toEqual({ paths: [INCOMING_PATH], position: { x: 5, y: 6 } })
    expect(sent[4].payload).toEqual({ paths: ['/.drops/1/a.txt'], position: { x: 9, y: 10 } })
    expect(fake.landed).toEqual([{ batch: 1, index: 0, text: 'hello' }])
    // Handed off as offered before the drop went out, so a transfer can claim the batch.
    expect(fake.handedOff).toEqual([{ batch: 1, offered: true }])
  })

  it('sends the keys again when they change, so Option still forces a copy', () => {
    setUp()
    window.dispatchEvent(drag('dragenter'))
    window.dispatchEvent(drag('dragover', { altKey: true }))
    expect(sent.filter((s) => s.event === 'drag-modifiers').map((s) => s.payload)).toEqual([
      { altHeld: false, cmdHeld: false, shiftHeld: false },
      { altHeld: true, cmdHeld: false, shiftHeld: false },
    ])
  })

  it('leaves when the drag leaves the page', () => {
    setUp()
    window.dispatchEvent(drag('dragenter'))
    window.dispatchEvent(drag('dragleave'))
    expect(names().at(-1)).toBe('tauri://drag-leave')
  })

  it('tells the page what the budget left out, and drops what came in', async () => {
    const refusal: ImportRefusal = { kind: 'overBudget', budgetBytes: 10_000_000, leftOut: 1 }
    setUp({ accepted: 1, roots: ['/.drops/1/a.txt'], refusal })
    window.dispatchEvent(drag('dragenter'))
    window.dispatchEvent(drag('drop', { files: [file('a.txt', 'a'), file('b.txt', 'b')] }))
    await settled()
    expect(refusals).toEqual([refusal])
  })

  it('leaves, with nothing to drop, when the budget took nothing', async () => {
    const refusal: ImportRefusal = { kind: 'overBudget', budgetBytes: 10_000_000, leftOut: 1 }
    setUp({ accepted: 0, roots: [], refusal })
    window.dispatchEvent(drag('dragenter'))
    window.dispatchEvent(drag('drop', { files: [file('big.bin', 'x')] }))
    await vi.waitFor(() => expect(refusals).toEqual([refusal]))
    await vi.waitFor(() => expect(names().at(-1)).toBe('tauri://drag-leave'))
    expect(names()).not.toContain('tauri://drag-drop')
    // Nothing went out, so the empty batch is swept at once.
    expect(fake.handedOff).toEqual([{ batch: 1, offered: false }])
  })

  it('ignores a drag without files, but keeps the browser from following it', () => {
    setUp()
    window.dispatchEvent(drag('dragenter', { types: ['text/uri-list'] }))
    const drop = drag('drop', { types: ['text/uri-list'] })
    window.dispatchEvent(drop)
    expect(drop.defaultPrevented).toBe(true)
    expect(sent).toEqual([])
  })
})
