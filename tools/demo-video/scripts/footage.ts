// Turns the recorded ProRes masters into H.264 proxies Remotion seeks quickly, and writes
// public/<videoId>/footage/manifest.json.
// Reads brand/clip-masters/<footageDir>/video-beats.json (`video` for the first video, `video2` for the second).
// Beats missing from it keep their still placeholders.
//
//   node scripts/footage.ts --video=anywhere            transcode what's new or changed (default video: demo)
//   node scripts/footage.ts --video=anywhere --force    transcode everything again
//
// Heavy: it encodes minutes of 1080p+ video. It runs at low priority with few threads, but don't run it while
// something CPU-sensitive (like a screen recording) is going.
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { paths } from '../src/config.ts'
import type { FootageManifest } from '../src/timeline.ts'
import { videoFromArgs } from '../src/videos/index.ts'
import { durationSec } from './lib/audio.ts'

const ROOT = join(import.meta.dirname, '..')
const video = videoFromArgs(process.argv)
const MANIFEST = resolve(ROOT, '../../brand/clip-masters', video.footageDir, 'video-beats.json')
const OUT = join(ROOT, 'public', paths(video.id).footage)
const force = process.argv.includes('--force')

type Loose = Record<string, unknown>
const str = (o: Loose, ...keys: string[]): string | undefined =>
  keys.map((k) => o[k]).find((v): v is string => typeof v === 'string')

/** Accepts a few plausible manifest shapes: { beats: {id: {...}} }, { beats: [...] }, or a bare array. */
function readBeats(json: unknown): { id: string; file: string; moments: Record<string, number> }[] {
  const root = json as Loose
  const raw = Array.isArray(json) ? json : (root.beats ?? root.clips ?? root)
  const entries: [string | undefined, Loose][] = Array.isArray(raw)
    ? raw.map((b: Loose) => [str(b, 'id', 'beat', 'name'), b])
    : Object.entries(raw as Record<string, Loose>)
  return entries.flatMap(([id, b]) => {
    const file = str(b, 'file', 'path', 'master', 'video', 'src')
    if (!id || !file || typeof b !== 'object') return []
    const moments: Record<string, number> = {}
    const m = b.moments ?? b.markers ?? b.marks
    if (Array.isArray(m))
      for (const x of m as Loose[]) {
        const name = str(x, 'name', 'label', 'id')
        const t = [x.t, x.time, x.at, x.seconds].find((v) => typeof v === 'number') as number | undefined
        if (name && t !== undefined) moments[name] = t
      }
    else if (m && typeof m === 'object')
      for (const [k, t] of Object.entries(m)) if (typeof t === 'number') moments[k] = t
    for (const k of ['start', 'in', 'trimStart'])
      if (typeof b[k] === 'number' && moments.start === undefined) moments.start = b[k] as number
    return [{ id, file, moments }]
  })
}

if (!existsSync(MANIFEST)) {
  console.log(`No ${MANIFEST} yet; the video keeps its still placeholders.`)
  process.exit(0)
}
mkdirSync(OUT, { recursive: true })
const manifest: FootageManifest = { beats: {} }
for (const beat of readBeats(JSON.parse(readFileSync(MANIFEST, 'utf8')))) {
  const src = resolve(dirname(MANIFEST), beat.file)
  if (!existsSync(src)) {
    console.warn(`${beat.id}: ${src} is missing, skipping`)
    continue
  }
  const file = `${paths(video.id).footage}/${beat.id}.mp4`
  const out = join(ROOT, 'public', file)
  if (force || !existsSync(out) || statSync(out).mtimeMs < statSync(src).mtimeMs) {
    console.log(`${beat.id}: transcoding ${beat.file}`)
    execFileSync(
      'nice',
      [
        '-n',
        '19',
        'ffmpeg',
        '-v',
        'error',
        '-y',
        '-threads',
        '4',
        '-i',
        src,
        '-vf',
        'scale=w=1920:h=1080:force_original_aspect_ratio=decrease:flags=lanczos,format=yuv420p',
        '-c:v',
        'libx264',
        '-preset',
        'slow',
        '-crf',
        '14',
        '-g',
        '15',
        '-bf',
        '0',
        '-an',
        '-movflags',
        '+faststart',
        out,
      ],
      { stdio: 'inherit' },
    )
  } else console.log(`${beat.id}: proxy is current`)
  manifest.beats[beat.id] = { file, duration: durationSec(out), moments: beat.moments }
}
writeFileSync(join(OUT, 'manifest.json'), JSON.stringify(manifest, null, 2))
console.log(`Wrote public/${paths(video.id).footage}/manifest.json with ${Object.keys(manifest.beats).length} beats`)
