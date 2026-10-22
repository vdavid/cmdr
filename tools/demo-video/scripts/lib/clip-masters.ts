// Reads a video's `brand/clip-masters/<footageDir>/video-beats.json`, as written by `pnpm marketing:clips --video`
// (apps/desktop/scripts/marketing-clips.ts).
import { existsSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { paths } from '../../src/config.ts'
import type { FootageBeat } from '../../src/timeline.ts'
import type { VideoSpec } from '../../src/videos/types.ts'

/** One beat in `video-beats.json`. Times are seconds from the master's start. */
export type MasterBeat = {
  id: string
  file: string
  duration: number
  width: number
  height: number
  usable: { start: number; end: number }
  keyFrame: number
  notes: { t: number; label: string }[]
  remark?: string
}

export const mastersManifestPath = (video: VideoSpec): string =>
  resolve(join(import.meta.dirname, '../../../../brand/clip-masters', video.footageDir, 'video-beats.json'))

export function readMasters(video: VideoSpec): MasterBeat[] | undefined {
  const path = mastersManifestPath(video)
  return existsSync(path) ? (JSON.parse(readFileSync(path, 'utf8')) as { beats: MasterBeat[] }).beats : undefined
}

/** The proxy's manifest entry: the notes become named moments, plus `start`/`end` (usable span) and `key`. */
export function footageEntry(video: VideoSpec, beat: MasterBeat, duration = beat.duration): FootageBeat {
  const moments: Record<string, number> = { start: beat.usable.start, end: beat.usable.end, key: beat.keyFrame }
  for (const n of beat.notes) moments[n.label] = n.t
  return {
    file: `${paths(video.id).footage}/${beat.id}.mp4`,
    duration,
    width: beat.width,
    height: beat.height,
    moments,
  }
}
