// Checks the mix: how far the voice sits above the music while someone talks, and the peak of the full mix.
// Render the stems first (audio only, cheap), then compare:
//   pnpm stems                      → out/<videoId>-stem-{voice,music,mix}.wav (both videos)
//   node scripts/check-levels.ts [--video=anywhere]   → the voice margin and the mix's peak
import { execFileSync } from 'node:child_process'
import { join } from 'node:path'
import { videoFromArgs } from '../src/videos/index.ts'
import { decodeMono, rmsWindows, toDb } from './lib/audio.ts'

const OUT = join(import.meta.dirname, '../out')
const id = videoFromArgs(process.argv).id
const rate = 16000
const w = 0.1
const voice = rmsWindows(decodeMono(join(OUT, `${id}-stem-voice.wav`), rate, 'highpass=f=100'), rate, w)
const music = rmsWindows(decodeMono(join(OUT, `${id}-stem-music.wav`), rate, 'highpass=f=100'), rate, w)
// The peak over every channel as-is (a mono downmix would misstate it).
const rawMix = execFileSync('ffmpeg', ['-v', 'error', '-i', join(OUT, `${id}-stem-mix.wav`), '-f', 'f32le', '-'], {
  maxBuffer: 1024 * 1024 * 1024,
})
const mix = new Float32Array(rawMix.buffer, rawMix.byteOffset, rawMix.byteLength / 4)
let peak = 0
for (const s of mix) peak = Math.max(peak, Math.abs(s))

const margins: { t: number; d: number }[] = []
for (let i = 0; i < Math.min(voice.length, music.length); i++) {
  const v = toDb(voice[i])
  // Only windows with real speech in them: word tails and breaths say nothing about intelligibility.
  if (v > -28) margins.push({ t: i * w, d: v - toDb(music[i]) })
}
margins.sort((a, b) => a.d - b.d)
const pct = (p: number): number => margins[Math.floor((margins.length - 1) * p)].d
console.log(
  `voice-over-music margin while talking (dB; windows with voice above -28 dB RMS; 100 Hz high-passed RMS, ${margins.length} windows):`,
)
console.log(`  worst ${pct(0).toFixed(1)}  5th pct ${pct(0.05).toFixed(1)}  median ${pct(0.5).toFixed(1)}`)
console.log(
  `  worst moments: ${margins
    .slice(0, 5)
    .map((m) => `${m.t.toFixed(1)} s (${m.d.toFixed(1)})`)
    .join(', ')}`,
)
console.log(`mix sample peak: ${toDb(peak).toFixed(2)} dBFS ${peak >= 0.999 ? '(CLIPPING)' : '(no clipping)'}`)
