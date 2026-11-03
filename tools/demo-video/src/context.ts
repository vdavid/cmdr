import { createContext, useContext } from 'react'
import { useCurrentFrame } from 'remotion'
import { FPS } from './config.ts'
import type { SectionPlan, Timeline, VoiceManifest } from './timeline.ts'
import type { VideoSpec } from './videos/types.ts'

type Ctx = { timeline: Timeline; voice: VoiceManifest; spec: VideoSpec }
export const TimelineContext = createContext<Ctx | null>(null)

export function useTimeline(): Ctx {
  const ctx = useContext(TimelineContext)
  if (!ctx) throw new Error('useTimeline outside TimelineContext')
  return ctx
}

/** Global time in seconds. Only meaningful in components rendered outside a <Sequence>. */
export const useT = (): number => useCurrentFrame() / FPS

export const sectionAt = (timeline: Timeline, t: number): SectionPlan =>
  timeline.sections.find((s) => t >= s.start && t < s.end) ?? timeline.sections[timeline.sections.length - 1]

/** The voice's loudness (0–1) at global time t, from the per-frame envelope; 0 between clips. */
export function voiceLevel(timeline: Timeline, voice: VoiceManifest, t: number): number {
  const clip = timeline.voice.find((c) => t >= c.start && t < c.start + (c.to - c.from))
  if (!clip) return 0
  const env = voice.sections[clip.sectionId].envelope
  const i = (t - clip.start + clip.from) * voice.fps
  const a = env[Math.floor(i)] ?? 0
  const b = env[Math.floor(i) + 1] ?? 0
  return a + (b - a) * (i - Math.floor(i))
}

/** Index of the phrase being spoken (or last spoken) in a section at global time t, or -1 before the first. */
export function phraseIndexAt(timeline: Timeline, sectionId: string, t: number, lead = 0.12): number {
  const caps = timeline.captions.filter((c) => c.sectionId === sectionId)
  let idx = -1
  for (const [i, c] of caps.entries()) if (t >= c.start - lead) idx = i
  return idx
}
