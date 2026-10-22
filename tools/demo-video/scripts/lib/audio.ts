// Audio helpers shared by the asset scripts. ffmpeg does the decoding; we only do arithmetic on raw samples.
import { execFileSync } from 'node:child_process'

/** Decodes any audio file to mono float samples at `rate` Hz, after an optional ffmpeg filter chain. */
export function decodeMono(path: string, rate: number, filter?: string): Float32Array {
  const af = [filter, `aresample=${rate}`].filter(Boolean).join(',')
  const buf = execFileSync('ffmpeg', ['-v', 'error', '-i', path, '-af', af, '-ac', '1', '-f', 'f32le', '-'], {
    maxBuffer: 1024 * 1024 * 1024,
  })
  return new Float32Array(buf.buffer, buf.byteOffset, buf.byteLength / 4)
}

/** RMS per window of `windowSec` seconds, as linear values. */
export function rmsWindows(samples: Float32Array, rate: number, windowSec: number): number[] {
  const n = Math.round(rate * windowSec)
  const out: number[] = []
  for (let i = 0; i < samples.length; i += n) {
    let sum = 0
    const end = Math.min(i + n, samples.length)
    for (let j = i; j < end; j++) sum += samples[j] * samples[j]
    out.push(Math.sqrt(sum / Math.max(1, end - i)))
  }
  return out
}

/**
 * Strong percussive hits (kick, snare, stabs), in seconds: peaks of the rising-energy curve that stand well above
 * their neighbourhood, at least `minGap` apart. What the warm theme's pulses ride.
 */
export function strongOnsets(path: string, minGap = 0.2): number[] {
  const rate = 8000
  const w = 0.01
  const env = rmsWindows(decodeMono(path, rate, 'lowpass=f=2500'), rate, w)
  const rise = env.map((v, i) => Math.max(0, v - (env[i - 1] ?? 0)))
  const sorted = [...rise].sort((a, b) => a - b)
  const floor = sorted[Math.floor(sorted.length * 0.9)]
  const near = Math.round(0.75 / w)
  const peakWin = Math.round(minGap / 2 / w)
  const out: number[] = []
  for (let i = 0; i < rise.length; i++) {
    if (rise[i] < floor) continue
    let local = 0
    for (let j = Math.max(0, i - near); j < Math.min(rise.length, i + near); j++) local += rise[j]
    local /= 2 * near
    let isPeak = rise[i] > 2.5 * local
    for (let j = Math.max(0, i - peakWin); isPeak && j <= Math.min(rise.length - 1, i + peakWin); j++)
      if (rise[j] > rise[i]) isPeak = false
    if (isPeak && (out.length === 0 || i * w - out[out.length - 1] >= minGap)) out.push(Math.round(i * w * 1000) / 1000)
  }
  return out
}

export const toDb = (x: number): number => (x > 0 ? 20 * Math.log10(x) : -120)

export function durationSec(path: string): number {
  const out = execFileSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', path])
  return Number(out.toString().trim())
}
