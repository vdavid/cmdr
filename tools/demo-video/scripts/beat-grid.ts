// Estimates a track's exact tempo and beat phase from onset autocorrelation, so music edits can jump by whole bars
// without a hiccup. Usage: node scripts/beat-grid.ts assets/music/<file> [minBpm=140] [maxBpm=150] [fromSec] [toSec]
// Prints the BPM, the time of beat 0, and where some candidate times fall on the grid.
import { decodeMono, rmsWindows } from './lib/audio.ts'

const [path, minArg = '140', maxArg = '150', fromArg = '40', toArg = '120', ...probes] = process.argv.slice(2)
if (!path)
  throw new Error('Usage: node scripts/beat-grid.ts <audio file> [minBpm] [maxBpm] [fromSec] [toSec] [probeSec...]')
const rate = 8000
const w = 0.005
const env = rmsWindows(decodeMono(path, rate), rate, w)
const onset = env.map((v, i) => Math.max(0, v - (env[i - 1] ?? 0)))
const at = (x: number): number => {
  const i = Math.floor(x)
  const f = x - i
  return (onset[i] ?? 0) * (1 - f) + (onset[i + 1] ?? 0) * f
}

const a = Math.round(Number(fromArg) / w)
const b = Math.round(Number(toArg) / w)
let bestBpm = 0
let bestScore = 0
for (let bpm = Number(minArg); bpm <= Number(maxArg); bpm += 0.005) {
  // Correlate at one bar (four beats) of lag: sharper than one beat for half-time dubstep.
  const lag = (4 * 60) / bpm / w
  let s = 0
  for (let i = a; i < b; i++) s += onset[i] * at(i + lag)
  if (s > bestScore) {
    bestScore = s
    bestBpm = bpm
  }
}
const beat = 60 / bestBpm

let phase = 0
let phaseScore = 0
for (let p = 0; p < beat; p += w) {
  let s = 0
  // Phase from the analyzed range only: live drums drift, so a whole-song phase can be off by a fraction of a beat.
  for (let t = p; t < Number(toArg); t += beat) if (t >= Number(fromArg)) s += at(t / w)
  if (s > phaseScore) {
    phaseScore = s
    phase = p
  }
}
console.log(
  `bpm ${bestBpm.toFixed(3)}  beat ${beat.toFixed(5)} s  bar ${(beat * 4).toFixed(5)} s  beat0 ${phase.toFixed(3)} s`,
)
for (const t of probes.map(Number)) console.log(`${t.toFixed(3)} s = beat ${((t - phase) / beat).toFixed(2)}`)
