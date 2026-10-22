/**
 * The staged folder tree every marketing clip shows: a small studio's shared drive with
 * nice, realistic, non-personal names. Created fresh by the clips run, ❌ never David's
 * real data.
 *
 * It lives at `/Users/Shared/Studio` (override: `CMDR_CLIPS_DEMO_ROOT`) because that path
 * reads as an ordinary folder in a breadcrumb and carries no user name, which a temp dir
 * (`/private/var/folders/…`) or a home path would.
 *
 * ❗ Safety: the tree is only ever deleted when it carries the `MARKER` file this module
 * wrote. A folder at that path WITHOUT the marker is somebody's real data, and staging
 * refuses to touch it. The E2E fixture guard is not in play here at all: the clips shard
 * never sets `CMDR_E2E_START_PATH` (`scripts/marketing-launch.ts` explains why).
 */

import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

export const DEMO_ROOT = process.env.CMDR_CLIPS_DEMO_ROOT ?? '/Users/Shared/Studio'

/** Proves a tree at `DEMO_ROOT` is ours to delete. Hidden, so no listing shows it. */
export const MARKER = '.cmdr-clips-demo' // allowed-stale-selector: a file name, not a selector

/** The folders the clips reach for by name. */
export const DEMO = {
  photos: 'Photos',
  trip: join('Photos', 'Iceland 2026'),
  rawImports: join('Photos', 'Raw imports'),
  backup: 'Backup',
  projects: 'Projects',
  documents: 'Documents',
  fieldNotes: join('Documents', 'Field notes'),
} as const

/** The projects the queue clip copies, one transfer each. */
export const QUEUE_SOURCES = [
  'Annual report',
  'Brand refresh',
  'Podcast season 3',
  'Website redesign',
  'Workshop',
] as const

/** The search clip's query, and the word its demo files are named after. */
export const SEARCH_QUERY = 'invoice'

/** How many photos the copy clip copies. Enough to leave room for Pause, Resume, and Rollback. */
const TRIP_PHOTOS = 64

/** How many files the multi-rename clip renames. A screenful in the preview. */
const RAW_IMPORTS = 18

/** A tiny deterministic PRNG (mulberry32), so every run stages the same sizes. */
function prng(seed: number): () => number {
  let a = seed
  return () => {
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/**
 * Scales every filler file. 1 in a run (the lists should show real-looking sizes); the
 * unit test shrinks it so staging twice doesn't write ~800 MB.
 */
const SIZE_SCALE = Number(process.env.CMDR_CLIPS_DEMO_SIZE_SCALE ?? '1')

/**
 * Writes `bytes` bytes of filler. Sizes are what the lists show; the content is never
 * looked at. Real bytes, ❌ never a sparse file: a sparse file has no allocated size, and
 * folder sizes would read near zero.
 */
function writeSized(path: string, bytes: number, byte = 0x5a): void {
  writeFileSync(path, Buffer.alloc(Math.max(1, Math.round(bytes * SIZE_SCALE)), byte))
}

/** A fixed moment in late summer 2026, `dayOffset` days in, so dates read naturally. */
function demoDate(dayOffset: number, minutes = 0): Date {
  return new Date(Date.UTC(2026, 7, 14 + dayOffset, 9, 30 + minutes))
}

function refuseForeignTree(root: string): void {
  if (existsSync(root) && !existsSync(join(root, MARKER))) {
    throw new Error(
      `${root} exists and isn't a clips demo tree (no ${MARKER} in it), so it may be real data. ` +
        'Move it away, or point CMDR_CLIPS_DEMO_ROOT somewhere else.',
    )
  }
}

/** Deletes the demo tree, and only a tree this module staged. */
export function removeDemoTree(root = DEMO_ROOT): void {
  refuseForeignTree(root)
  rmSync(root, { recursive: true, force: true })
}

/** (Re)creates the whole demo tree from scratch. */
export function stageDemoTree(root = DEMO_ROOT): void {
  removeDemoTree(root)
  mkdirSync(root, { recursive: true })
  writeFileSync(join(root, MARKER), 'Staged by pnpm marketing:clips. Safe to delete.\n')

  stageTripPhotos(root)
  resetRawImports(root)
  mkdirSync(join(root, DEMO.backup), { recursive: true })
  stageProjects(root)
  stageDocuments(root)

  const music = join(root, 'Music')
  mkdirSync(music, { recursive: true })
  const rand = prng(11)
  for (const name of ['Lowlands.flac', 'Northern lights.flac', 'Slow tide.flac', 'Westfjords.flac']) {
    writeSized(join(music, name), 18_000_000 + Math.floor(rand() * 14_000_000))
  }
}

function stageTripPhotos(root: string): void {
  const dir = join(root, DEMO.trip)
  mkdirSync(dir, { recursive: true })
  const rand = prng(3)
  for (let i = 0; i < TRIP_PHOTOS; i++) {
    const path = join(dir, `IMG_${String(4021 + i * 3)}.jpg`)
    writeSized(path, 2_400_000 + Math.floor(rand() * 3_800_000))
    utimesSync(path, demoDate(Math.floor(i / 9), i * 7), demoDate(Math.floor(i / 9), i * 7))
  }
}

/** Puts the multi-rename clip's files back to their camera names, for the next take. */
export function resetRawImports(root = DEMO_ROOT): void {
  const dir = join(root, DEMO.rawImports)
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  const rand = prng(5)
  for (let i = 0; i < RAW_IMPORTS; i++) {
    const path = join(dir, `DSC_${String(7310 + i * 4).padStart(4, '0')}.jpg`)
    writeSized(path, 3_100_000 + Math.floor(rand() * 2_900_000))
    // Spread over three days, so the date in the new names changes partway down the list.
    const when = demoDate(Math.floor(i / 6), i * 11)
    utimesSync(path, when, when)
  }
}

/** Empties `Backup/`, where the copy and queue clips land, for the next take. */
export function resetBackup(root = DEMO_ROOT): void {
  const dir = join(root, DEMO.backup)
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
}

function stageProjects(root: string): void {
  const rand = prng(7)
  const kinds = ['.psd', '.pdf', '.key', '.png', '.docx', '.mp3', '.txt'] // allowed-stale-selector: file extensions
  for (const [p, project] of QUEUE_SOURCES.entries()) {
    const dir = join(root, DEMO.projects, project)
    mkdirSync(dir, { recursive: true })
    for (let i = 0; i < 16; i++) {
      const ext = kinds[(i + p) % kinds.length]
      writeSized(join(dir, `${project} ${String(i + 1).padStart(2, '0')}${ext}`), 40_000 + Math.floor(rand() * 900_000))
    }
  }
  // The search clip's hits live in more than one folder, so the results show paths.
  writeSized(join(root, DEMO.projects, 'Website redesign', 'Invoice 2026-041 Website redesign.pdf'), 182_000)
  writeSized(join(root, DEMO.projects, 'Podcast season 3', 'Invoice 2026-052 Podcast mixing.pdf'), 96_000)
}

function stageDocuments(root: string): void {
  const invoices = join(root, DEMO.documents, 'Invoices')
  mkdirSync(invoices, { recursive: true })
  for (let month = 1; month <= 9; month++) {
    writeSized(join(invoices, `Invoice 2026-0${String(month)}0.pdf`), 120_000 + month * 7_300)
  }
  writeSized(join(root, DEMO.documents, 'Invoice template.docx'), 48_000)
  writeFileSync(join(root, DEMO.documents, 'Budget 2026.csv'), 'Month,Income,Costs\nJan,12400,8100\nFeb,13900,8650\n')

  const notes = join(root, DEMO.fieldNotes)
  mkdirSync(notes, { recursive: true })
  writeFileSync(join(notes, 'README.md'), FIELD_NOTES_README)
  writeFileSync(join(notes, 'packing-list.txt'), PACKING_LIST)
  // Bytes that look like a real binary in the hex view: a header, then a pattern.
  const firmware = Buffer.alloc(64 * 1024)
  for (let i = 0; i < firmware.length; i++) firmware[i] = (i * 31 + (i >> 8)) & 0xff
  Buffer.from('CMDRFW\u0001\u0000Studio light controller v2.4\u0000').copy(firmware)
  writeFileSync(join(notes, 'light-controller.bin'), firmware)
  stagePhoto(join(notes, 'glacier.jpg'))
}

/**
 * A real JPEG for the viewer to render: a deterministic ImageMagick plasma, which reads
 * as a landscape-ish photo at a glance. The clips run requires ImageMagick up front.
 */
function stagePhoto(path: string): void {
  const res = spawnSync(
    'magick',
    ['-size', '1600x1000', '-seed', '2026', 'plasma:#9fd3e6-#1d3557', '-blur', '0x2', '-quality', '88', path],
    { encoding: 'utf8' },
  )
  if (res.error !== undefined || res.status !== 0) {
    throw new Error(`magick couldn't render the demo photo: ${res.stderr || String(res.error)}`)
  }
}

const FIELD_NOTES_README = `# Field notes

Two weeks around the Westfjords, August 2026.

![The edge of Drangajökull](glacier.jpg)

## What's in here

- [Full-size photo](glacier.jpg): the one above, at full resolution
- [Packing list](packing-list.txt): what came along, and what we'd leave home next time
- [Light controller firmware](light-controller.bin): for the studio lamp we took on the road

## Highlights

1. Dynjandi in the rain
2. Puffins at Látrabjarg
3. The hot pot at Hellulaug, after dark
`

const PACKING_LIST = `Packing list, Westfjords 2026
=============================

Clothes
  - Rain shell and rain trousers
  - Wool base layers (2 sets)
  - Hiking boots, sandals for the hot pots

Camera
  - Body, 24-70 and 70-200
  - Three batteries, two cards
  - Lens cloths (bring more than you think)

Car
  - Paper map of the fjords
  - Thermos, oat bars, dried fish

Leave home next time
  - The second tripod
  - The drone (wind, every day)
`
