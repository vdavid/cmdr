/**
 * Records and encodes the website's product clips, in one command.
 *
 * ```bash
 * pnpm marketing:clips                        # every clip, both themes, into apps/website/public/clips/
 * pnpm marketing:clips --only copy-rollback   # one clip (repeat or comma-separate for more)
 * pnpm marketing:clips --out /tmp/clips       # write somewhere else, leave the website alone
 * pnpm marketing:clips --encode-only          # re-encode the masters already in brand/clip-masters/
 * pnpm marketing:clips --dry-run              # check the machine, launch nothing
 * pnpm marketing:clips --remove-demo          # delete the demo tree the runs keep in /Users/Shared/Studio
 * ```
 *
 * Three stages:
 *
 * 1. **Record**: launches a prod-looking app on the clips data dir (`marketing-launch.ts`,
 *    shared with `pnpm marketing:shots`) and runs `marketing-clips.spec.ts`, which stages
 *    a demo tree, drives each clip's real UI actions, and records the window through the
 *    ScreenCaptureKit recorder (`marketing-clips-recorder.swift`, compiled here on demand)
 *    into lossless masters in `brand/clip-masters/` (gitignored).
 * 2. **Encode**: trims each master to the spec's markers and writes AV1, H.264, and a
 *    WebP poster per clip and theme (`marketing-clips-encode.ts`).
 * 3. **Manifest**: `clips.json` beside them, one line per clip with its size and length.
 *
 * ❗ Leave the machine alone while it records: each take brings Cmdr to the front, and a
 * window behind another app renders frozen frames (WKWebView starves an occluded window).
 */

import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import {
  accessSync,
  constants,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { ensureE2eBinary, warnIfForeignCmdr } from './capture-runtime.ts'
import {
  CLIP_THEMES,
  encodeClip,
  mergeClipsManifest,
  missingEncodeTools,
  probeVideo,
  readClipsManifest,
  writeClipsManifest,
} from './marketing-clips-encode.ts'
import type { ClipManifestEntry, ClipMarkers } from './marketing-clips-encode.ts'
import { captureInstance, launchCaptureApp, prepareCaptureInstance, runCaptureShard } from './marketing-launch.ts'
import { CLIP_SLUGS, VIDEO_BEATS, VIDEO_TITLE_PREFIX } from '../test/e2e-playwright/marketing-clips-slugs.ts'
import type { ClipSlug, VideoBeat } from '../test/e2e-playwright/marketing-clips-slugs.ts'
import { buildVideoManifest } from './marketing-video.ts'
import { DEMO_ROOT, MARKER, ensureDemoTree, removeDemoTree } from '../test/e2e-playwright/marketing-clips-demo.ts'

const LOG = '[marketing-clips]'

const here = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(here, '..', '..', '..')
const RECORDER_SOURCE = join(here, 'marketing-clips-recorder.swift')
/** Compiled recorders, keyed by source hash, so every worktree shares one build per version. */
const RECORDER_CACHE = join(homedir(), 'Library', 'Caches', 'com.veszelovszki.cmdr-clips', 'recorder')

interface Options {
  /** Clip slugs, or with `--video` beat ids. */
  only: string[]
  outDir: string
  mastersDir: string
  dryRun: boolean
  encodeOnly: boolean
  removeDemo: boolean
  /** The demo video's long-form beats instead of the clips: dark only, masters and a manifest, no encode. */
  video: boolean
}

function parseArgs(argv: string[]): Options {
  const options: Options = {
    only: [],
    outDir: join(repoRoot, 'apps', 'website', 'public', 'clips'),
    mastersDir: '',
    dryRun: false,
    encodeOnly: false,
    removeDemo: false,
    video: false,
  }
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    const value = (): string => {
      const next = argv.at(i + 1)
      if (next === undefined || next.startsWith('--')) throw new Error(`${arg} needs a value`)
      i++
      return next
    }
    if (arg === '--only') options.only.push(...value().split(','))
    else if (arg === '--out') options.outDir = value()
    else if (arg === '--masters') options.mastersDir = value()
    else if (arg === '--dry-run') options.dryRun = true
    else if (arg === '--encode-only') options.encodeOnly = true
    else if (arg === '--remove-demo') options.removeDemo = true
    else if (arg === '--video') options.video = true
    else throw new Error(`unknown argument '${arg}'`)
  }
  return validated(options)
}

/** Checks what only the whole command line can tell, and fills in the defaults that depend on it. */
function validated(options: Options): Options {
  const known: readonly string[] = options.video ? VIDEO_BEATS : CLIP_SLUGS
  for (const name of options.only) {
    if (!known.includes(name))
      throw new Error(`unknown ${options.video ? 'beat' : 'clip'} '${name}'. Known: ${known.join(', ')}`)
  }
  if (options.video && options.encodeOnly) throw new Error('--video records masters only; there is nothing to encode')
  if (options.mastersDir === '') {
    options.mastersDir = join(repoRoot, 'brand', 'clip-masters', ...(options.video ? ['video'] : []))
  }
  return options
}

/** The clip slugs this run covers. */
function selectedSlugs(options: Options): ClipSlug[] {
  return options.only.length > 0 ? (options.only as ClipSlug[]) : [...CLIP_SLUGS]
}

/** The video beats this run covers. */
function selectedBeats(options: Options): VideoBeat[] {
  return options.only.length > 0 ? (options.only as VideoBeat[]) : [...VIDEO_BEATS]
}

/** What the editor must know about a beat that its footage doesn't say. */
const BEAT_REMARKS: Partial<Record<VideoBeat, string>> = {
  ai:
    'A regular filename search, NOT an AI one: no cloud AI provider was configured for the recording instance. ' +
    'Re-record once one is set up in its Settings > AI > Provider.',
}

/**
 * The recorder binary for the current source, compiled when no build of this exact
 * source exists yet. ~10 s the first time, free after.
 */
function ensureRecorder(): string {
  const hash = createHash('sha256').update(readFileSync(RECORDER_SOURCE)).digest('hex').slice(0, 16)
  const binary = join(RECORDER_CACHE, `cmdr-clip-recorder-${hash}`)
  if (existsSync(binary)) return binary
  mkdirSync(RECORDER_CACHE, { recursive: true })
  console.log(`${LOG} compiling the recorder (once per version of its source)…`)
  const building = `${binary}.building`
  const res = spawnSync('swiftc', ['-O', '-swift-version', '5', '-parse-as-library', '-o', building, RECORDER_SOURCE], {
    encoding: 'utf8',
  })
  if (res.error !== undefined || res.status !== 0) {
    rmSync(building, { force: true })
    throw new Error(`swiftc couldn't build the recorder: ${res.stderr.trim() || String(res.error)}`)
  }
  renameSync(building, binary)
  return binary
}

/** One environment check: what it is, and why it failed (null when it passed). */
interface Check {
  name: string
  problem: string | null
}

/**
 * Everything the run needs that can be checked without launching the app or recording
 * anything: the encoders, the recorder's build and its Screen Recording access, System
 * Events automation (the take brings the app to the front through it), and a free demo
 * folder. The dry run prints these; a real run refuses on the first failure.
 */
function preflight(options: Options): { checks: Check[]; recorder: string | null } {
  const checks: Check[] = []
  const check = (name: string, run: () => string | null): void => {
    try {
      checks.push({ name, problem: run() })
    } catch (e) {
      checks.push({ name, problem: e instanceof Error ? e.message : String(e) })
    }
  }

  check('macOS', () => (process.platform === 'darwin' ? null : 'the recorder is ScreenCaptureKit, macOS only'))
  check('ffmpeg (libsvtav1, libx264), ffprobe, ImageMagick', () => {
    const missing = missingEncodeTools()
    return missing.length === 0 ? null : `missing: ${missing.join(', ')}`
  })

  let recorder: string | null = null
  if (!options.encodeOnly) {
    check('the recorder compiles (swiftc)', () => {
      recorder = ensureRecorder()
      return null
    })
    check('Screen Recording permission (CGPreflightScreenCaptureAccess + SCShareableContent)', () => {
      if (recorder === null) return 'no recorder to ask'
      const res = spawnSync(recorder, ['--preflight'], { encoding: 'utf8', timeout: 20000 })
      if (res.status === 0) return null
      return (
        `${res.stdout.trim() || res.stderr.trim() || `exit ${String(res.status)}`}. Grant Screen Recording to the ` +
        'terminal running this (System Settings > Privacy & Security > Screen Recording), then restart it.'
      )
    })
    check('System Events automation (brings the app to the front)', () => {
      const res = spawnSync('osascript', ['-e', 'tell application "System Events" to count processes'], {
        encoding: 'utf8',
        timeout: 20000,
      })
      return res.status === 0
        ? null
        : `${res.stderr.trim()}. Allow the terminal to control System Events (Privacy & Security > Automation).`
    })
    check(`the demo folder (${DEMO_ROOT})`, () => {
      if (existsSync(DEMO_ROOT) && !existsSync(join(DEMO_ROOT, MARKER))) {
        return 'it exists and is not a staged demo tree, so it may be real data; move it or set CMDR_CLIPS_DEMO_ROOT'
      }
      accessSync(dirname(DEMO_ROOT), constants.W_OK)
      return null
    })
  }
  return { checks, recorder }
}

/** The masters on disk for `slug` and `theme`, with their markers, or why there aren't any. */
function masterFor(
  mastersDir: string,
  slug: ClipSlug,
  theme: (typeof CLIP_THEMES)[number],
  notBefore: number,
): { master: string; markers: ClipMarkers } | string {
  const master = join(mastersDir, `${slug}-${theme}.mov`)
  const markersPath = join(mastersDir, `${slug}-${theme}.markers.json`)
  if (!existsSync(master) || !existsSync(markersPath)) return 'no master recorded'
  // A master from an earlier run is not this run's result; ship it only when asked to.
  if (statSync(markersPath).mtimeMs < notBefore) return 'its take failed this run (an older master is on disk)'
  return { master, markers: JSON.parse(readFileSync(markersPath, 'utf8')) as ClipMarkers }
}

async function record(options: Options, recorder: string): Promise<boolean> {
  warnIfForeignCmdr(LOG)
  const binary = ensureE2eBinary(LOG)
  const instance = captureInstance('clips')
  mkdirSync(options.mastersDir, { recursive: true })
  prepareCaptureInstance(instance, LOG, {})
  // Before the launch, so the app's startup journal replay indexes a freshly staged tree
  // (`ensureDemoTree` explains why a tree staged under a running app reads `<dir>`).
  console.log(`${LOG} demo tree at ${DEMO_ROOT}: ${ensureDemoTree()}`)

  const app = await launchCaptureApp({
    binary,
    instance,
    logPrefix: LOG,
    extraEnv: { CMDR_CLIPS_MASTERS_DIR: options.mastersDir, CMDR_CLIPS_RECORDER: recorder },
  })
  console.log(`${LOG} socket ready; recording…`)
  const grep = options.video
    ? `${VIDEO_TITLE_PREFIX}(${selectedBeats(options).join('|')}) dark$`
    : `(${selectedSlugs(options).join('|')}) (${CLIP_THEMES.join('|')})$`
  try {
    runCaptureShard('marketing-clips', { ...app.sharedEnv, CMDR_SHOTS_PID: String(app.pid) }, ['--grep', grep])
    return true
  } catch (e) {
    // Some takes failed; the ones that succeeded are still worth encoding. Say so and go on.
    console.error(`${LOG} ${e instanceof Error ? e.message : String(e)}; encoding the takes that did record.`)
    return false
  } finally {
    app.stop()
  }
}

function encodeAll(options: Options, notBefore: number): string[] {
  mkdirSync(options.outDir, { recursive: true })
  const fresh: ClipManifestEntry[] = []
  const skipped: string[] = []
  for (const slug of selectedSlugs(options)) {
    for (const theme of CLIP_THEMES) {
      const found = masterFor(options.mastersDir, slug, theme, notBefore)
      if (typeof found === 'string') {
        skipped.push(`${slug}-${theme}: ${found}`)
        continue
      }
      console.log(`${LOG} encoding ${slug}-${theme}…`)
      fresh.push(encodeClip({ ...found, outDir: options.outDir }))
    }
  }
  // One theme each merges into one line per clip.
  const merged = new Map<string, ClipManifestEntry>()
  for (const entry of fresh) {
    const before = merged.get(entry.slug)
    merged.set(entry.slug, before === undefined ? entry : { ...entry, themes: [...before.themes, ...entry.themes] })
  }
  const manifestPath = join(options.outDir, 'clips.json')
  writeClipsManifest(manifestPath, mergeClipsManifest(readClipsManifest(manifestPath), [...merged.values()]))
  console.log(`${LOG} ${String(fresh.length)} clip files encoded into ${options.outDir}.`)
  return skipped
}

/** Writes `video-beats.json` beside the video masters, and returns the beats it couldn't list. */
function writeVideoManifest(options: Options, notBefore: number): string[] {
  const { manifest, missing } = buildVideoManifest({
    mastersDir: options.mastersDir,
    beats: selectedBeats(options),
    notBefore,
    remarks: BEAT_REMARKS,
    probe: probeVideo,
  })
  const path = join(options.mastersDir, 'video-beats.json')
  // An `--only` run keeps the other beats' entries.
  const before = existsSync(path) ? (JSON.parse(readFileSync(path, 'utf8')) as typeof manifest).beats : []
  const fresh = new Set(manifest.beats.map((b) => b.id))
  const beats = [...before.filter((b) => !fresh.has(b.id)), ...manifest.beats].sort(
    (a, b) => VIDEO_BEATS.indexOf(a.id as VideoBeat) - VIDEO_BEATS.indexOf(b.id as VideoBeat),
  )
  writeFileSync(path, `${JSON.stringify({ ...manifest, beats }, null, 2)}\n`)
  console.log(`${LOG} ${String(manifest.beats.length)} beat masters listed in ${path}.`)
  return missing.map((beat) => `${beat}: no fresh master`)
}

/** What a dry run says it would do. */
function printPlan(options: Options): void {
  if (options.video) {
    console.log(`${LOG} would record the demo video's beats: ${selectedBeats(options).join(', ')} (dark)`)
    console.log(`${LOG} masters and video-beats.json: ${options.mastersDir}`)
    return
  }
  const slugs = selectedSlugs(options)
  console.log(
    `${LOG} would ${options.encodeOnly ? 're-encode' : 'record and encode'}: ${slugs.join(', ')} (dark and light)`,
  )
  console.log(`${LOG} masters: ${options.mastersDir}`)
  console.log(`${LOG} output: ${options.outDir} ({slug}-{theme}-av1.mp4, -h264.mp4, .webp, clips.json)`)
  if (!options.encodeOnly) {
    console.log(`${LOG} the E2E binary is built first if the tree changed (not checked by a dry run).`)
  }
}

async function main(): Promise<void> {
  const options = parseArgs(process.argv.slice(2))
  if (options.removeDemo) {
    removeDemoTree()
    console.log(`${LOG} removed the demo tree at ${DEMO_ROOT}.`)
    return
  }
  const started = Date.now()

  const { checks, recorder } = preflight(options)
  for (const { name, problem } of checks)
    console.log(`${LOG} ${problem === null ? '✓' : '✗'} ${name}${problem === null ? '' : `: ${problem}`}`)
  const failed = checks.filter((c) => c.problem !== null)

  if (options.dryRun) printPlan(options)
  if (failed.length > 0) throw new Error(`${String(failed.length)} check(s) failed; see above`)
  if (options.dryRun) {
    console.log(`${LOG} dry run OK: nothing launched, nothing recorded.`)
    return
  }

  let recordedAll = true
  if (!options.encodeOnly) {
    if (recorder === null) throw new Error('unreachable: the preflight passed without a recorder')
    recordedAll = await record(options, recorder)
  }

  const skipped = options.video
    ? writeVideoManifest(options, started)
    : encodeAll(options, options.encodeOnly ? 0 : started)
  for (const line of skipped) console.warn(`${LOG} skipped ${line}`)
  if (!recordedAll || skipped.length > 0) throw new Error('not every clip was recorded; see the skipped lines above')
  console.log(`${LOG} done. Review the clips before committing them: they're drafts until David has seen them.`)
}

main()
  .then(() => process.exit(0))
  .catch((e: unknown) => {
    console.error(`${LOG} ${e instanceof Error ? e.message : String(e)}`)
    process.exit(1)
  })
