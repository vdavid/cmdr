// Prints a video's computed edit: section cuts, voice clips, music segments (with track times), and the total length.
// Usage: node scripts/timeline.ts [--video=anywhere] [--footage-plan]
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { paths } from '../src/config.ts'
import { buildTimeline, type FootageManifest, type VoiceManifest } from '../src/timeline.ts'
import { videoFromArgs } from '../src/videos/index.ts'
import { footageEntry, readMasters } from './lib/clip-masters.ts'

const video = videoFromArgs(process.argv)
const p = paths(video.id)
const pub = join(import.meta.dirname, '../public')
const read = <T>(rel: string, fallback?: T): T => {
  const file = join(pub, rel)
  if (existsSync(file)) return JSON.parse(readFileSync(file, 'utf8')) as T
  if (fallback !== undefined) return fallback
  throw new Error(`Missing public/${rel}; see README.md`)
}
const voice = read<VoiceManifest>(`${p.voice}/manifest.json`)
// `--footage-plan`: plan against the masters' manifest directly, before (or without) transcoding the proxies.
const masters = process.argv.includes('--footage-plan') ? readMasters(video) : undefined
const footage: FootageManifest = masters
  ? { beats: Object.fromEntries(masters.map((b) => [b.id, footageEntry(video, b)])) }
  : read<FootageManifest>(`${p.footage}/manifest.json`, { beats: {} })
const onsets = read<number[]>(p.onsets, [])
const t = buildTimeline(video, voice, footage, onsets)
const s = (x: number): string => x.toFixed(2).padStart(6)

console.log(
  `${video.id}: total ${s(t.duration)} s   drop1 ${s(t.drop1)}   lift ${t.bigDrop === null ? '  none' : s(t.bigDrop)}   final hit ${s(t.finalHit)}   pulses ${t.pulses.length}`,
)
for (const sec of t.sections)
  console.log(
    `section ${sec.id.padEnd(9)} ${s(sec.start)} → ${s(sec.end)}  (${(sec.end - sec.start).toFixed(2)} s)  shots: ${sec.shots.map((x) => (x.footage ? 'video' : 'still')).join(', ') || '-'}`,
  )
for (const sec of t.sections)
  for (const sh of sec.shots) {
    if (!sh.footage) continue
    const fr = sh.footage.from
    const to = fr + (sh.end - sh.start)
    const holdIn = Math.max(0, sh.footage.usableStart - fr)
    const holdOut = Math.max(0, to - sh.footage.duration)
    console.log(
      `  shot  ${sec.id.padEnd(9)} ${s(sh.start)} → ${s(sh.end)}  clip ${s(fr)} → ${s(to)}${holdIn > 0 ? `  holds ${holdIn.toFixed(2)} s first` : ''}${holdOut > 0 ? `  freezes ${holdOut.toFixed(2)} s at the end` : ''}${sh.syncAt !== undefined ? `  sync at ${sh.syncAt.toFixed(2)}` : ''}`,
    )
  }
for (const c of t.voice)
  console.log(
    `voice   ${c.sectionId.padEnd(9)} ${s(c.start)} → ${s(c.start + c.to - c.from)}  (file ${c.from.toFixed(2)}–${c.to.toFixed(2)})`,
  )
for (const m of t.music)
  console.log(`music   ${s(m.start)} → ${s(m.end)}  track ${s(m.start + m.trackOffset)} → ${s(m.end + m.trackOffset)}`)
