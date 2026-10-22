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

export const toDb = (x: number): number => (x > 0 ? 20 * Math.log10(x) : -120)

export function durationSec(path: string): number {
  const out = execFileSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', path])
  return Number(out.toString().trim())
}
