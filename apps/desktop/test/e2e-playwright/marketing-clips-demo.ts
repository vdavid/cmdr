/**
 * The staged folder tree every marketing clip shows: a small studio's shared drive with
 * nice, realistic, non-personal names, ❌ never David's real data. Staged by the clips
 * run and kept between runs (`ensureDemoTree` says why); `pnpm marketing:clips
 * --remove-demo` deletes it.
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
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
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
  /** The demo video's speed beat: one folder holding `SENSOR_LOGS` files. */
  sensorLogs: join('Archive', 'Sensor logs'),
  /** The demo video's rename beat. */
  cameraRoll: join('Photos', 'Camera roll'),
  code: 'Code',
  /** A real git repo, for the `.git` portal. */
  repo: join('Code', 'website'),
  zip: join('Code', 'Brand assets.zip'),
} as const

/** The speed beat's file count (scaled down with `SIZE_SCALE` in the unit test). */
export const SENSOR_LOGS = 50_000

/** The file the speed beat searches for among the sensor logs. */
export const SENSOR_LOG_QUERY = 'sensor-41873'

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

/** How many photos the demo video's rename beat renames. */
const CAMERA_ROLL = 30

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

/**
 * What the marker says. Bump the version whenever the staged content changes, so the next
 * run restages instead of keeping a stale tree.
 */
const MARKER_TEXT = 'Staged by pnpm marketing:clips (demo tree v3). Safe to delete.\n'

/**
 * Makes sure the demo tree is there and current, and puts its mutable folders back.
 *
 * ❗ It KEEPS a current tree rather than restaging it every run, and the orchestrator calls
 * it BEFORE the app launches. A tree deleted and recreated under a running app comes back
 * without folder sizes: the index's verifier marks the old one stale and the new subtree
 * reads `<dir>` for minutes ("Parent path not found in index", first real run,
 * 2026-10-22). Staged before launch, the startup journal replay indexes it like any
 * other change.
 */
export function ensureDemoTree(root = DEMO_ROOT): 'kept' | 'staged' {
  const marker = join(root, MARKER)
  if (existsSync(marker) && readFileSync(marker, 'utf8') === MARKER_TEXT) {
    resetRawImports(root)
    resetCameraRoll(root)
    resetBackup(root)
    return 'kept'
  }
  stageDemoTree(root)
  return 'staged'
}

/** (Re)creates the whole demo tree from scratch. Prefer `ensureDemoTree`, which says why. */
export function stageDemoTree(root = DEMO_ROOT): void {
  removeDemoTree(root)
  mkdirSync(root, { recursive: true })
  writeFileSync(join(root, MARKER), MARKER_TEXT)

  stageTripPhotos(root)
  resetRawImports(root)
  mkdirSync(join(root, DEMO.backup), { recursive: true })
  stageProjects(root)
  stageDocuments(root)
  resetCameraRoll(root)
  stageSensorLogs(root)
  stageCode(root)

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
  emptyDir(dir)
  const rand = prng(5)
  for (let i = 0; i < RAW_IMPORTS; i++) {
    const path = join(dir, `DSC_${String(7310 + i * 4).padStart(4, '0')}.jpg`)
    writeSized(path, 3_100_000 + Math.floor(rand() * 2_900_000))
    // Spread over three days, so the date in the new names changes partway down the list.
    const when = demoDate(Math.floor(i / 6), i * 11)
    utimesSync(path, when, when)
  }
}

/** Puts the demo video's rename beat's photos back to their camera names. */
export function resetCameraRoll(root = DEMO_ROOT): void {
  const dir = join(root, DEMO.cameraRoll)
  emptyDir(dir)
  const rand = prng(13)
  for (let i = 0; i < CAMERA_ROLL; i++) {
    const path = join(dir, `IMG_${String(5102 + i * 2)}.jpg`)
    writeSized(path, 2_800_000 + Math.floor(rand() * 3_400_000))
    const when = demoDate(4 + Math.floor(i / 10), i * 13)
    utimesSync(path, when, when)
  }
}

/**
 * The speed beat's big folder: `SENSOR_LOGS` small log files with zero-padded numbers,
 * so it sorts like a real data dump and one exact name is findable.
 */
function stageSensorLogs(root: string): void {
  const dir = join(root, DEMO.sensorLogs)
  mkdirSync(dir, { recursive: true })
  const count = Math.max(50, Math.round(SENSOR_LOGS * SIZE_SCALE))
  const rand = prng(17)
  for (let i = 1; i <= count; i++) {
    writeFileSync(join(dir, `sensor-${String(i).padStart(5, '0')}.log`), 'x'.repeat(200 + Math.floor(rand() * 3800)))
  }
}

/** A small website repo with real history (for the `.git` portal) and a zip beside it. */
function stageCode(root: string): void {
  const repo = join(root, DEMO.repo)
  mkdirSync(join(repo, 'src'), { recursive: true })
  const git = (args: string[], day: number): void => {
    const date = `2026-0${String(6 + Math.floor(day / 28))}-${String((day % 28) + 1).padStart(2, '0')}T10:00:00Z`
    const res = spawnSync('git', args, {
      cwd: repo,
      encoding: 'utf8',
      env: {
        ...process.env,
        GIT_AUTHOR_NAME: 'Studio',
        GIT_AUTHOR_EMAIL: 'studio@example.com',
        GIT_COMMITTER_NAME: 'Studio',
        GIT_COMMITTER_EMAIL: 'studio@example.com',
        GIT_AUTHOR_DATE: date,
        GIT_COMMITTER_DATE: date,
      },
    })
    if (res.status !== 0) throw new Error(`git ${args.join(' ')} failed: ${res.stderr}`)
  }
  const commit = (message: string, day: number, files: Record<string, string>): void => {
    for (const [name, text] of Object.entries(files)) writeFileSync(join(repo, name), text)
    git(['add', '.'], day)
    git(['commit', '-q', '-m', message], day)
  }
  git(['init', '-q', '-b', 'main'], 0)
  commit('First page', 0, { 'index.html': '<h1>Studio</h1>\n', 'README.md': '# Studio website\n' })
  commit('Add styles', 3, { 'styles.css': 'body { font-family: system-ui; }\n' })
  commit('Gallery page', 9, { 'gallery.html': '<h1>Gallery</h1>\n', 'src/gallery.js': 'export const photos = []\n' })
  git(['tag', 'v1.0'], 9)
  git(['checkout', '-q', '-b', 'redesign'], 12)
  commit('New color palette', 12, { 'styles.css': 'body { font-family: system-ui; color: #1d3557; }\n' })
  git(['checkout', '-q', 'main'], 14)
  commit('Contact form', 15, { 'contact.html': '<form></form>\n', 'src/contact.js': 'export function send() {}\n' })

  // The zip: the brand refresh project's files, browsed like a folder in the reach beat.
  const res = spawnSync('zip', ['-q', '-r', join(root, DEMO.zip), 'Brand refresh'], {
    cwd: join(root, DEMO.projects),
    encoding: 'utf8',
  })
  if (res.status !== 0) throw new Error(`zip failed: ${res.stderr}`)
}

/** Empties `Backup/`, where the copy and queue clips land, for the next take. */
export function resetBackup(root = DEMO_ROOT): void {
  emptyDir(join(root, DEMO.backup))
}

/**
 * Empties `dir` but keeps the folder itself, so the index never sees it deleted and
 * recreated (see `ensureDemoTree`).
 */
function emptyDir(dir: string): void {
  mkdirSync(dir, { recursive: true })
  for (const entry of readdirSync(dir)) rmSync(join(dir, entry), { recursive: true, force: true })
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
 * A real JPEG for the viewer to render: a calm, drawn landscape (dusk sky, two mountain
 * ridges, a lake), deterministic so every run shows the same picture. A plasma fill read
 * as noise rather than a photo (first real run, 2026-10-22). The clips run requires
 * ImageMagick up front.
 */
function stagePhoto(path: string): void {
  const res = spawnSync(
    'magick',
    [
      '-size',
      '1600x1000',
      'gradient:#3d5a80-#f2c48d',
      '-fill',
      '#fff4d6',
      '-draw',
      'circle 1180,420 1180,375',
      '-fill',
      '#5c6b8a',
      '-draw',
      'polygon 0,640 180,470 330,560 520,380 760,600 980,450 1210,610 1420,500 1600,580 1600,760 0,760',
      '-fill',
      '#2f3b52',
      '-draw',
      'polygon 0,720 260,580 470,690 700,560 940,700 1180,600 1400,720 1600,650 1600,780 0,780',
      '-fill',
      '#1c2638',
      '-draw',
      'rectangle 0,760 1600,1000',
      '-fill',
      '#e9b98066',
      '-draw',
      'rectangle 1140,790 1220,800',
      '-blur',
      '0x1.2',
      '-quality',
      '88',
      path,
    ],
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
