// Prints a video's computed edit: section cuts, voice clips, music segments (with track times), and the total length.
// Usage: node scripts/timeline.ts [--video=anywhere]
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { paths } from '../src/config.ts'
import { buildTimeline, type FootageManifest, type VoiceManifest } from '../src/timeline.ts'
import { videoFromArgs } from '../src/videos/index.ts'

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
const footage = read<FootageManifest>(`${p.footage}/manifest.json`, { beats: {} })
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
for (const c of t.voice)
  console.log(
    `voice   ${c.sectionId.padEnd(9)} ${s(c.start)} → ${s(c.start + c.to - c.from)}  (file ${c.from.toFixed(2)}–${c.to.toFixed(2)})`,
  )
for (const m of t.music)
  console.log(`music   ${s(m.start)} → ${s(m.end)}  track ${s(m.start + m.trackOffset)} → ${s(m.end + m.trackOffset)}`)
