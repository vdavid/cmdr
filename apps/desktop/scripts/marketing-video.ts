/**
 * The demo video's footage manifest: `video-beats.json`, beside the long-form masters that
 * `pnpm marketing:clips --video` records into `brand/clip-masters/video/`.
 *
 * One entry per beat: its file, length, and pixel size, where the usable footage starts
 * and ends inside the master (the slack around it is for the editor's cuts), and the
 * scene's named moments, each as seconds from the master's first frame. Pure apart from
 * the probe, and unit tested (`marketing-video.test.ts`).
 */

import { existsSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

/** What a take writes beside its master (`marketing-clips-take.ts`), notes included. */
export interface BeatMarkers {
  slug: string
  firstFrameEpochMs: number
  startEpochMs: number
  endEpochMs: number
  posterEpochMs: number
  notes?: { epochMs: number; label: string }[]
}

export interface BeatEntry {
  id: string
  file: string
  /** The master's length, in seconds. */
  duration: number
  width: number
  height: number
  /** The usable footage, in seconds from the master's start; outside it is slack. */
  usable: { start: number; end: number }
  /** The single frame that explains the beat best, in seconds from the master's start. */
  keyFrame: number
  notes: { t: number; label: string }[]
  /** Anything the editor must know that the footage doesn't say. */
  remark?: string
}

export interface VideoManifest {
  generatedBy: string
  theme: 'dark'
  fps: number
  codec: string
  beats: BeatEntry[]
}

/** Seconds from the master's first frame, to the millisecond. */
function since(first: number, at: number): number {
  return Math.round(Math.max(0, at - first)) / 1000
}

/** One beat's entry, from its markers and its probed master. */
export function beatEntry(
  markers: BeatMarkers,
  probe: { duration: number; width: number; height: number },
  file: string,
  remark?: string,
): BeatEntry {
  const first = markers.firstFrameEpochMs
  return {
    id: markers.slug,
    file,
    duration: Math.round(probe.duration * 1000) / 1000,
    width: probe.width,
    height: probe.height,
    usable: { start: since(first, markers.startEpochMs), end: since(first, markers.endEpochMs) },
    keyFrame: since(first, markers.posterEpochMs),
    notes: (markers.notes ?? []).map((note) => ({ t: since(first, note.epochMs), label: note.label })),
    ...(remark === undefined ? {} : { remark }),
  }
}

/** The beats' entries, in `beats` order; a beat with no fresh master is reported, not listed. */
export function buildVideoManifest(options: {
  mastersDir: string
  beats: readonly string[]
  notBefore: number
  remarks: Record<string, string>
  probe: (path: string) => { duration: number; width: number; height: number }
}): { manifest: VideoManifest; missing: string[] } {
  const entries: BeatEntry[] = []
  const missing: string[] = []
  for (const beat of options.beats) {
    const file = `${beat}-dark.mov`
    const master = join(options.mastersDir, file)
    const markersPath = join(options.mastersDir, `${beat}-dark.markers.json`)
    if (!existsSync(master) || !existsSync(markersPath) || statSync(markersPath).mtimeMs < options.notBefore) {
      missing.push(beat)
      continue
    }
    const markers = JSON.parse(readFileSync(markersPath, 'utf8')) as BeatMarkers
    entries.push(beatEntry(markers, options.probe(master), file, options.remarks[beat]))
  }
  return {
    manifest: {
      generatedBy: 'pnpm marketing:clips --video (apps/desktop/scripts/marketing-clips.ts)',
      theme: 'dark',
      fps: 60,
      codec: 'ProRes 4444, variable frame rate up to 60 fps (frames arrive only when the window changes)',
      beats: entries,
    },
    missing,
  }
}
