// Prints a loudness profile of a music track (full band and bass), to find drops and clean hits when re-editing
// the music. Usage: node scripts/analyze-music.ts assets/music/<file> [windowSec=0.5] [fromSec] [toSec]
import { decodeMono, rmsWindows, toDb } from './lib/audio.ts'

const [path, windowArg = '0.5', fromArg = '0', toArg = '9999'] = process.argv.slice(2)
if (!path) throw new Error('Usage: node scripts/analyze-music.ts <audio file> [windowSec] [fromSec] [toSec]')
const windowSec = Number(windowArg)
const rate = 8000
const full = rmsWindows(decodeMono(path, rate), rate, windowSec)
const bass = rmsWindows(decodeMono(path, rate, 'lowpass=f=150,lowpass=f=150'), rate, windowSec)

const bar = (db: number): string => '#'.repeat(Math.max(0, Math.round((db + 50) / 1.5)))
for (let i = 0; i < full.length; i++) {
  const t = i * windowSec
  if (t < Number(fromArg) || t > Number(toArg)) continue
  const f = toDb(full[i])
  const b = toDb(bass[i])
  console.log(
    `${t.toFixed(2).padStart(7)}  full ${f.toFixed(1).padStart(6)}  bass ${b.toFixed(1).padStart(6)}  ${bar(b)}`,
  )
}
