/**
 * The encode half of `pnpm marketing:clips`: turns one lossless master (`.mov`, ProRes
 * 4444, from `marketing-clips-recorder.swift`) plus its markers into the three files the
 * website ships, and keeps `clips.json` describing them.
 *
 * Per clip and theme, into the output dir:
 *
 * - `{slug}-{theme}-av1.mp4`: SVT-AV1 in screen-content mode, the primary source.
 * - `{slug}-{theme}-h264.mp4`: the fallback for browsers without AV1 decode (Safari
 *   decodes AV1 only in hardware: M3+, iPhone 15 Pro+).
 * - `{slug}-{theme}.webp`: the poster frame, shown before play and under
 *   `prefers-reduced-motion`.
 *
 * The planning (`planTrim`, `ffmpegVideoArgs`, `mergeClipsManifest`) is pure and unit
 * tested; `encodeClip` shells out to `ffmpeg`, `ffprobe`, and `magick` and is tested on a
 * synthetic `testsrc2` master (`marketing-clips-encode.test.ts`).
 */

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** Frame rate every encode lands on: half the master's 60, plenty for UI motion. */
export const OUTPUT_FPS = 30

/** Widest a clip ships. ~1,500 px reads sharp on a retina laptop at the layout's width. */
export const MAX_OUTPUT_WIDTH = 1500

/**
 * Seconds of the last frame held past a still ending before the output's `-t` cuts it.
 * The master already ends at its STOP time (the recorder ends the session there), so
 * this is a margin against the `fps` filter dropping a final variable-length frame.
 */
const TAIL_PAD_S = 2

export type ClipTheme = 'dark' | 'light'
export const CLIP_THEMES: readonly ClipTheme[] = ['dark', 'light']

/**
 * What the spec writes beside each master, `{slug}-{theme}.markers.json`. Every time is a
 * wall-clock `Date.now()` value; the recorder reports its first frame on the same clock.
 */
export interface ClipMarkers {
  slug: string
  theme: ClipTheme
  /** When the master's first frame was captured, so t=0 of the `.mov`. */
  firstFrameEpochMs: number
  /** Where the clip starts and ends. */
  startEpochMs: number
  endEpochMs: number
  /** The representative moment the poster frame is taken from. */
  posterEpochMs: number
}

/** Offsets into the master, in seconds, for one encode. */
export interface TrimPlan {
  startS: number
  durationS: number
  /** The poster's offset from the clip's START (not from the master's t=0). */
  posterS: number
}

/**
 * Turns wall-clock markers into offsets into the master.
 *
 * Refuses markers that can't describe a clip rather than encoding a wrong one: an end
 * before its start, or a start before the first frame by more than a frame's worth of
 * clock jitter (the recorder and the spec read the same wall clock, but through two
 * processes). The poster clamps into the clip.
 */
export function planTrim(markers: ClipMarkers): TrimPlan {
  const jitterMs = 50
  if (markers.startEpochMs < markers.firstFrameEpochMs - jitterMs) {
    throw new Error(
      `${markers.slug}-${markers.theme}: the start marker is ${String(markers.firstFrameEpochMs - markers.startEpochMs)} ms ` +
        'before the recording began, so the master has nothing there',
    )
  }
  if (markers.endEpochMs <= markers.startEpochMs) {
    throw new Error(`${markers.slug}-${markers.theme}: the end marker isn't after the start marker`)
  }
  const startS = Math.max(0, (markers.startEpochMs - markers.firstFrameEpochMs) / 1000)
  const durationS = (markers.endEpochMs - markers.startEpochMs) / 1000
  const posterS = Math.min(durationS, Math.max(0, (markers.posterEpochMs - markers.startEpochMs) / 1000))
  return { startS: round3(startS), durationS: round3(durationS), posterS: round3(posterS) }
}

function round3(n: number): number {
  return Math.round(n * 1000) / 1000
}

/** The output width for a master `sourceWidth` px wide: the cap, never an upscale, always even. */
export function outputWidth(sourceWidth: number): number {
  return Math.floor(Math.min(MAX_OUTPUT_WIDTH, sourceWidth) / 2) * 2
}

/**
 * The shared filter chain: cut, re-time to zero, pad the tail, resample to 30 fps, scale
 * with Lanczos (text survives it best), and convert to BT.709 4:2:0, the one format
 * every browser decodes for both codecs.
 */
export function videoFilter(plan: TrimPlan, width: number): string {
  return [
    `trim=start=${String(plan.startS)}`,
    'setpts=PTS-STARTPTS',
    `tpad=stop_mode=clone:stop_duration=${String(TAIL_PAD_S)}`,
    `fps=${String(OUTPUT_FPS)}`,
    `scale=${String(width)}:-2:flags=lanczos+accurate_rnd+full_chroma_int:out_color_matrix=bt709:out_range=tv`,
    'format=yuv420p',
  ].join(',')
}

/** BT.709 tags, so a browser doesn't guess the matrix and shift the brand colors. */
const COLOR_TAGS = ['-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv']

/**
 * `ffmpeg` arguments for one video encode of `master` into `out`.
 *
 * - **AV1**: SVT-AV1 with screen-content tools (`scm=1`: palette and intra block copy,
 *   built for flat UI and text). CRF 42 at preset 4: text stays as crisp as the H.264's
 *   at roughly 70–85% of its bytes. CRF 32 came out LARGER than the H.264 on every clip,
 *   and `scm=1` beat both `scm=0` and auto by 5–10% (measured on the viewer clip, SVT-AV1
 *   4.2.0, 2026-10-23). Real UI clips land at roughly 250–900 KB, above the 150–500 KB the
 *   landing page was planned around (issue #693), because 2x masters of text-heavy
 *   windows carry more detail than that estimate assumed.
 * - **H.264**: High profile, CRF 22, `faststart` so playback starts before the last byte.
 *
 * Both are silent (`-an`) and `faststart`, and both get the same filter chain, so the
 * two files are frame-for-frame the same picture.
 */
export function ffmpegVideoArgs(
  master: string,
  out: string,
  plan: TrimPlan,
  width: number,
  codec: 'av1' | 'h264',
): string[] {
  const codecArgs =
    codec === 'av1'
      ? ['-c:v', 'libsvtav1', '-preset', '4', '-crf', '42', '-svtav1-params', 'scm=1:tune=0']
      : ['-c:v', 'libx264', '-preset', 'veryslow', '-crf', '22', '-profile:v', 'high', '-tune', 'animation']
  return [
    '-hide_banner',
    '-loglevel',
    'error',
    '-y',
    '-i',
    master,
    '-vf',
    videoFilter(plan, width),
    '-t',
    String(plan.durationS),
    '-an',
    ...codecArgs,
    ...COLOR_TAGS,
    '-movflags',
    '+faststart',
    out,
  ]
}

/** `ffmpeg` arguments that pull the poster frame out of the master as a PNG. */
export function ffmpegPosterArgs(master: string, out: string, plan: TrimPlan, width: number): string[] {
  return [
    '-hide_banner',
    '-loglevel',
    'error',
    '-y',
    '-ss',
    String(round3(plan.startS + plan.posterS)),
    '-i',
    master,
    '-frames:v',
    '1',
    '-vf',
    `scale=${String(width)}:-2:flags=lanczos+accurate_rnd+full_chroma_int`,
    out,
  ]
}

/** One clip's line in `clips.json`. Both themes share a geometry: the same take, re-themed. */
export interface ClipManifestEntry {
  slug: string
  width: number
  height: number
  /** Seconds, to the millisecond. */
  duration: number
  themes: ClipTheme[]
}

export interface ClipsManifest {
  generatedBy: string
  clips: ClipManifestEntry[]
}

/**
 * Folds freshly encoded clips into the existing manifest: an `--only` run must keep
 * every other clip's line. A clip's themes merge (one theme re-encoded keeps the other),
 * and the list stays sorted by slug so a regeneration diffs cleanly.
 */
export function mergeClipsManifest(existing: ClipsManifest | null, fresh: ClipManifestEntry[]): ClipsManifest {
  const bySlug = new Map((existing?.clips ?? []).map((clip) => [clip.slug, clip]))
  for (const clip of fresh) {
    const before = bySlug.get(clip.slug)
    const themes = new Set([...(before?.themes ?? []), ...clip.themes])
    bySlug.set(clip.slug, { ...clip, themes: CLIP_THEMES.filter((theme) => themes.has(theme)) })
  }
  return {
    generatedBy: 'pnpm marketing:clips (apps/desktop/scripts/marketing-clips.ts)',
    clips: [...bySlug.values()].sort((a, b) => a.slug.localeCompare(b.slug)),
  }
}

export function readClipsManifest(path: string): ClipsManifest | null {
  if (!existsSync(path)) return null
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as ClipsManifest
  } catch {
    return null
  }
}

export function writeClipsManifest(path: string, manifest: ClipsManifest): void {
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`)
}

/** The encoders and tools the encode needs, and which of them this machine lacks. */
export function missingEncodeTools(): string[] {
  const missing: string[] = []
  const encoders = spawnSync('ffmpeg', ['-hide_banner', '-encoders'], { encoding: 'utf8' })
  if (encoders.error !== undefined || encoders.status !== 0) {
    missing.push('ffmpeg')
  } else {
    if (!/\blibsvtav1\b/.test(encoders.stdout)) missing.push('ffmpeg with libsvtav1')
    if (!/\blibx264\b/.test(encoders.stdout)) missing.push('ffmpeg with libx264')
  }
  const probe = spawnSync('ffprobe', ['-version'], { encoding: 'utf8' })
  if (probe.error !== undefined || probe.status !== 0) missing.push('ffprobe')
  const magick = spawnSync('magick', ['-version'], { encoding: 'utf8' })
  if (magick.error !== undefined || magick.status !== 0) missing.push('ImageMagick (magick)')
  return missing
}

/** A video file's first stream: its size, codec, and duration in seconds. */
export function probeVideo(path: string): { width: number; height: number; codec: string; duration: number } {
  const res = spawnSync(
    'ffprobe',
    [
      '-v',
      'error',
      '-select_streams',
      'v:0',
      '-show_entries',
      'stream=width,height,codec_name:format=duration',
      '-of',
      'json',
      path,
    ],
    { encoding: 'utf8' },
  )
  if (res.status !== 0) throw new Error(`ffprobe couldn't read ${path}: ${res.stderr.trim()}`)
  const parsed = JSON.parse(res.stdout) as {
    streams: { width: number; height: number; codec_name: string }[]
    format: { duration: string }
  }
  const [stream] = parsed.streams
  return {
    width: stream.width,
    height: stream.height,
    codec: stream.codec_name,
    duration: Number(parsed.format.duration),
  }
}

function runTool(cmd: string, args: string[]): void {
  const res = spawnSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'inherit', 'pipe'] })
  if (res.error !== undefined || res.status !== 0) {
    throw new Error(`${cmd} failed: ${res.stderr.trim() || String(res.error ?? res.status)}`)
  }
}

/**
 * Encodes one master into `outDir` and returns its manifest line (one theme).
 *
 * Writes each output under a temp name and renames it in only once all three exist, so
 * a failed encode never leaves a half-set where a good one was.
 */
export function encodeClip(options: { master: string; markers: ClipMarkers; outDir: string }): ClipManifestEntry {
  const { master, markers, outDir } = options
  const plan = planTrim(markers)
  const width = outputWidth(probeVideo(master).width)
  const base = join(outDir, `${markers.slug}-${markers.theme}`)
  const av1 = `${base}-av1.mp4`
  const h264 = `${base}-h264.mp4`
  const poster = `${base}.webp`
  const posterPng = `${base}.poster.png`
  // The extension stays last so ffmpeg and magick still infer the container.
  const finals = [av1, h264, poster]
  const staged = finals.map((path) => path.replace(/(\.\w+)$/, '.encoding$1'))

  try {
    runTool('ffmpeg', ffmpegVideoArgs(master, staged[0], plan, width, 'av1'))
    runTool('ffmpeg', ffmpegVideoArgs(master, staged[1], plan, width, 'h264'))
    runTool('ffmpeg', ffmpegPosterArgs(master, posterPng, plan, width))
    // Lossy WebP: the poster is on screen for a moment before the video, and a q88
    // frame is a fraction of the lossless one with no visible difference at that size.
    runTool('magick', [posterPng, '-quality', '88', '-define', 'webp:method=6', staged[2]])
    for (const [i, path] of finals.entries()) renameSync(staged[i], path)
  } finally {
    for (const path of [...staged, posterPng]) rmSync(path, { force: true })
  }

  const probed = probeVideo(h264)
  return {
    slug: markers.slug,
    width: probed.width,
    height: probed.height,
    duration: round3(probed.duration),
    themes: [markers.theme],
  }
}
