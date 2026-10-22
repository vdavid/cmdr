/**
 * Tests for the clip encode in `marketing-clips-encode.ts`.
 *
 * The planning is pure and tested exhaustively. The real encode is tested end to end on
 * a synthetic master (`ffmpeg -f lavfi testsrc2`, written as ProRes 4444 like the
 * recorder's), so a bad filter chain or a codec flag the installed ffmpeg rejects fails
 * HERE rather than at the end of an overnight recording run. That half skips on a
 * machine without ffmpeg + libsvtav1 + libx264 + ImageMagick (CI's Linux runners).
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  encodeClip,
  ffmpegVideoArgs,
  mergeClipsManifest,
  missingEncodeTools,
  outputWidth,
  planTrim,
  probeVideo,
  videoFilter,
} from './marketing-clips-encode.ts'
import type { ClipMarkers } from './marketing-clips-encode.ts'

function markers(overrides: Partial<ClipMarkers> = {}): ClipMarkers {
  return {
    slug: 'copy-rollback',
    theme: 'dark',
    firstFrameEpochMs: 1_000_000,
    startEpochMs: 1_000_500,
    endEpochMs: 1_008_500,
    posterEpochMs: 1_003_000,
    ...overrides,
  }
}

describe('planTrim', () => {
  it('maps wall-clock markers onto offsets into the master', () => {
    expect(planTrim(markers())).toEqual({ startS: 0.5, durationS: 8, posterS: 2.5 })
  })

  it('absorbs a start a few ms before the first frame (two processes, one clock)', () => {
    expect(planTrim(markers({ startEpochMs: 999_980 })).startS).toBe(0)
  })

  it('refuses a start the master has no frames for', () => {
    expect(() => planTrim(markers({ startEpochMs: 990_000 }))).toThrow(/before the recording began/)
  })

  it('refuses an end that is not after the start', () => {
    expect(() => planTrim(markers({ endEpochMs: 1_000_500 }))).toThrow(/end marker/)
  })

  it('clamps the poster into the clip', () => {
    expect(planTrim(markers({ posterEpochMs: 2_000_000 })).posterS).toBe(8)
    expect(planTrim(markers({ posterEpochMs: 0 })).posterS).toBe(0)
  })
})

describe('outputWidth', () => {
  it('caps at 1,500 px and never upscales', () => {
    expect(outputWidth(2000)).toBe(1500)
    expect(outputWidth(1280)).toBe(1280)
  })

  it('stays even, which 4:2:0 needs', () => {
    expect(outputWidth(1281)).toBe(1280)
  })
})

describe('ffmpegVideoArgs', () => {
  const plan = { startS: 1.25, durationS: 6, posterS: 2 }

  it('puts AV1 in screen-content mode and keeps both codecs silent and faststart', () => {
    const av1 = ffmpegVideoArgs('in.mov', 'out.mp4', plan, 1500, 'av1')
    expect(av1).toContain('libsvtav1')
    expect(av1[av1.indexOf('-svtav1-params') + 1]).toContain('scm=1')
    const h264 = ffmpegVideoArgs('in.mov', 'out.mp4', plan, 1500, 'h264')
    expect(h264).toContain('libx264')
    for (const args of [av1, h264]) {
      expect(args).toContain('-an')
      expect(args[args.indexOf('-movflags') + 1]).toBe('+faststart')
      expect(args[args.indexOf('-t') + 1]).toBe('6')
      expect(args.at(-1)).toBe('out.mp4')
    }
  })

  it('trims, resamples to 30 fps, and lands on 4:2:0', () => {
    const filter = videoFilter(plan, 1500)
    expect(filter.startsWith('trim=start=1.25,setpts=PTS-STARTPTS')).toBe(true)
    expect(filter).toContain('fps=30')
    expect(filter).toContain('scale=1500:-2')
    expect(filter.endsWith('format=yuv420p')).toBe(true)
  })
})

describe('mergeClipsManifest', () => {
  it('keeps clips an --only run did not touch, and sorts by slug', () => {
    const existing = {
      generatedBy: 'x',
      clips: [{ slug: 'viewer', width: 1500, height: 960, duration: 8, themes: ['dark' as const, 'light' as const] }],
    }
    const merged = mergeClipsManifest(existing, [
      { slug: 'search', width: 1500, height: 960, duration: 6, themes: ['dark'] },
    ])
    expect(merged.clips.map((c) => c.slug)).toEqual(['search', 'viewer'])
  })

  it('merges themes, so re-encoding one keeps the other', () => {
    const existing = {
      generatedBy: 'x',
      clips: [{ slug: 'search', width: 1500, height: 960, duration: 6, themes: ['light' as const] }],
    }
    const merged = mergeClipsManifest(existing, [
      { slug: 'search', width: 1500, height: 960, duration: 6.5, themes: ['dark'] },
    ])
    expect(merged.clips).toEqual([
      { slug: 'search', width: 1500, height: 960, duration: 6.5, themes: ['dark', 'light'] },
    ])
  })
})

const tools = missingEncodeTools()

describe.skipIf(tools.length > 0)('encodeClip on a synthetic master', () => {
  let dir = ''
  let master = ''

  beforeAll(() => {
    dir = mkdtempSync(join(tmpdir(), 'cmdr-clips-encode-'))
    master = join(dir, 'search-dark.mov')
    // Four seconds of 60 fps 2x-ish footage in the recorder's own intermediate format.
    const res = spawnSync(
      'ffmpeg',
      [
        '-hide_banner',
        '-loglevel',
        'error',
        '-f',
        'lavfi',
        '-i',
        'testsrc2=size=1800x1100:rate=60',
        '-t',
        '4',
        '-c:v',
        'prores_ks',
        '-profile:v',
        '4444',
        '-pix_fmt',
        'yuva444p10le',
        master,
      ],
      { encoding: 'utf8' },
    )
    expect(res.status, res.stderr).toBe(0)
  }, 60_000)

  afterAll(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  it('writes AV1, H.264, and a poster at the capped width and the marked duration', () => {
    const entry = encodeClip({
      master,
      markers: markers({
        slug: 'search',
        firstFrameEpochMs: 0,
        startEpochMs: 500,
        endEpochMs: 3000,
        posterEpochMs: 1500,
      }),
      outDir: dir,
    })

    expect(entry).toMatchObject({ slug: 'search', width: 1500, height: 916, themes: ['dark'] })
    expect(entry.duration).toBeGreaterThan(2.4)
    expect(entry.duration).toBeLessThan(2.6)

    const av1 = probeVideo(join(dir, 'search-dark-av1.mp4'))
    expect(av1.codec).toBe('av1')
    expect(av1.width).toBe(1500)
    expect(probeVideo(join(dir, 'search-dark-h264.mp4')).codec).toBe('h264')

    const poster = join(dir, 'search-dark.webp')
    expect(existsSync(poster)).toBe(true)
    expect(statSync(poster).size).toBeGreaterThan(0)
    // Only the three deliverables (plus the master) are left behind.
    expect(existsSync(join(dir, 'search-dark.poster.png'))).toBe(false)
  }, 120_000)
})
