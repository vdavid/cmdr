// Finds `music.finalJumpNudge`: live drums drift, so a jump of whole bars by the average tempo can land a little off
// the beat. This correlates the drum envelope around the jump's source and destination and reports the shift that
// lines them up. Put the result in the video spec, then rerun to confirm it reports ~0.
// Usage: node scripts/align-jump.ts --video=anywhere
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { paths } from '../src/config.ts'
import { buildTimeline, type VoiceManifest } from '../src/timeline.ts'
import { videoFromArgs } from '../src/videos/index.ts'
import { decodeMono, rmsWindows } from './lib/audio.ts'

const video = videoFromArgs(process.argv)
const ROOT = join(import.meta.dirname, '..')
const manifest = join(ROOT, 'public', paths(video.id).voice, 'manifest.json')
if (!existsSync(manifest)) throw new Error(`Run pnpm voice --video=${video.id} first`)
const voice = JSON.parse(readFileSync(manifest, 'utf8')) as VoiceManifest
const t = buildTimeline(video, voice, {})
const rate = 8000
const w = 0.005
const env = rmsWindows(decodeMono(join(ROOT, 'assets/music', video.music.source), rate, 'lowpass=f=2500'), rate, w)
const onset = env.map((v, i) => Math.max(0, v - (env[i - 1] ?? 0)))
const span = 4
const score = (src: number, dst: number, shift: number): number => {
  let s = 0
  for (let x = -span; x <= span; x += w)
    s += (onset[Math.round((src + x) / w)] ?? 0) * (onset[Math.round((dst + shift + x) / w)] ?? 0)
  return s
}
const beat = 60 / video.music.bpm
// Every join in the edit: the lift jump (if any), then the ending jump.
for (let i = 1; i < t.music.length; i++) {
  const jump = t.music[i].start
  const src = jump + t.music[i - 1].trackOffset
  const dst = jump + t.music[i].trackOffset
  let best = 0
  let bestScore = -1
  for (let shift = -beat / 2; shift <= beat / 2; shift += w) {
    const s = score(src, dst, shift)
    if (s > bestScore) {
      bestScore = s
      best = shift
    }
  }
  const last = i === t.music.length - 1
  console.log(
    `${video.id}: ${last ? 'ending' : 'lift'} jump at video ${jump.toFixed(2)} s, track ${src.toFixed(2)} → ${dst.toFixed(2)} s`,
  )
  console.log(
    `  best alignment: shift the destination by ${best.toFixed(3)} s (score ${bestScore.toFixed(3)}, unshifted ${score(src, dst, 0).toFixed(3)})`,
  )
  if (last) console.log(`  set music.finalJumpNudge to ${(video.music.finalJumpNudge + best).toFixed(3)}`)
}
