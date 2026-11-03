// Turns the recorded ProRes masters into H.264 proxies Remotion seeks quickly, and writes
// public/<videoId>/footage/manifest.json. Reads brand/clip-masters/<footageDir>/video-beats.json (`video` for the
// first video, `video2` for the second). Beats missing from it keep their still placeholders.
//
//   node scripts/footage.ts --video=anywhere            transcode what's new or changed (default video: demo)
//   node scripts/footage.ts --video=anywhere --force    transcode everything again
//
// Heavy: it encodes minutes of 2560 × 1600 video. It runs at low priority with few threads, but don't run it while
// something CPU-sensitive (like a screen recording) is going. To check the edit without encoding anything:
// `pnpm timeline --video=<id> --footage-plan`.
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { FPS, paths } from '../src/config.ts'
import type { FootageManifest } from '../src/timeline.ts'
import { videoFromArgs } from '../src/videos/index.ts'
import { durationSec } from './lib/audio.ts'
import { footageEntry, mastersManifestPath, readMasters } from './lib/clip-masters.ts'

const ROOT = join(import.meta.dirname, '..')
const video = videoFromArgs(process.argv)
const OUT = join(ROOT, 'public', paths(video.id).footage)
const force = process.argv.includes('--force')

const beats = readMasters(video)
if (!beats) {
  console.log(`No ${mastersManifestPath(video)} yet; the video keeps its still placeholders.`)
  process.exit(0)
}
mkdirSync(OUT, { recursive: true })
const manifest: FootageManifest = { beats: {} }
for (const beat of beats) {
  const src = resolve(dirname(mastersManifestPath(video)), beat.file)
  if (!existsSync(src)) {
    console.warn(`${beat.id}: ${src} is missing, skipping`)
    continue
  }
  if (beat.remark) console.warn(`${beat.id}: note from the recording: ${beat.remark}`)
  const entry = footageEntry(video, beat)
  const out = join(ROOT, 'public', entry.file)
  if (force || !existsSync(out) || statSync(out).mtimeMs < statSync(src).mtimeMs) {
    console.log(`${beat.id}: transcoding ${beat.file}`)
    // Full resolution (the punch-ins need the pixels), constant frame rate at the render's own rate (the masters are
    // variable-rate), and short GOPs without B-frames so Remotion seeks fast.
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
        `fps=${FPS},scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p`,
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
  manifest.beats[beat.id] = { ...entry, duration: durationSec(out) }
}
writeFileSync(join(OUT, 'manifest.json'), JSON.stringify(manifest, null, 2))
console.log(`Wrote public/${paths(video.id).footage}/manifest.json with ${Object.keys(manifest.beats).length} beats`)
