// Prints the computed edit: section cuts, voice clips, music segments (with track times), and the total length.
// Usage: node scripts/timeline.ts
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { buildTimeline, type FootageManifest, type VoiceManifest } from '../src/timeline.ts'

const pub = join(import.meta.dirname, '../public')
const voice: VoiceManifest = JSON.parse(readFileSync(join(pub, 'voice/manifest.json'), 'utf8'))
const footagePath = join(pub, 'footage/manifest.json')
const footage: FootageManifest = existsSync(footagePath) ? JSON.parse(readFileSync(footagePath, 'utf8')) : { beats: {} }
const t = buildTimeline(voice, footage)
const s = (x: number): string => x.toFixed(2).padStart(6)

console.log(`total ${s(t.duration)} s   drop1 ${s(t.drop1)}   big drop ${s(t.bigDrop)}   final hit ${s(t.finalHit)}`)
for (const sec of t.sections)
  console.log(
    `section ${sec.id.padEnd(7)} ${s(sec.start)} → ${s(sec.end)}  (${(sec.end - sec.start).toFixed(2)} s)  shots: ${sec.shots.map((x) => (x.footage ? 'video' : 'still')).join(', ') || '-'}`,
  )
for (const c of t.voice)
  console.log(
    `voice   ${c.sectionId.padEnd(7)} ${s(c.start)} → ${s(c.start + c.to - c.from)}  (file ${c.from.toFixed(2)}–${c.to.toFixed(2)})`,
  )
for (const m of t.music)
  console.log(`music   ${s(m.start)} → ${s(m.end)}  track ${s(m.start + m.trackOffset)} → ${s(m.end + m.trackOffset)}`)
