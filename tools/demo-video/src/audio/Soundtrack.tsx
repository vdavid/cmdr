import { Audio, Sequence, staticFile } from 'remotion'
import { FPS, paths } from '../config.ts'
import { useTimeline } from '../context.ts'
import { f, type Caption, type Timeline } from '../timeline.ts'
import type { MusicSpec } from '../videos/types.ts'

/** Spans where the voice talks, with short gaps merged so the music doesn't pump between phrases. */
function speechSpans(captions: Caption[]): [number, number][] {
  const spans: [number, number][] = []
  for (const c of [...captions].sort((a, b) => a.start - b.start)) {
    const s = c.start - 0.15
    const e = c.end + 0.2
    const last = spans[spans.length - 1]
    if (last && s - last[1] < 0.75) last[1] = Math.max(last[1], e)
    else spans.push([s, e])
  }
  return spans
}

const smoothstep = (x: number): number => {
  const c = Math.min(1, Math.max(0, x))
  return c * c * (3 - 2 * c)
}

/** Music gain at global time t: ducked under speech, open between lines, full for the final hit. */
export function musicGain(music: MusicSpec, timeline: Timeline, spans: [number, number][], t: number): number {
  let d = Infinity
  for (const [a, b] of spans) d = Math.min(d, t < a ? a - t : t > b ? t - b : 0)
  const open = music.gainOpen
  let g = music.gainUnderVoice + (open - music.gainUnderVoice) * smoothstep(d / 0.35)
  // The intro sits a little lower so the sting and the first words feel intimate, then the first drop opens up.
  if (t < timeline.drop1 - 0.1) g *= 0.75
  // Punch windows: each drop hits fully open, however close the surrounding lines are, then eases back under the
  // next line over a short ramp.
  for (const drop of [timeline.drop1, timeline.bigDrop]) {
    if (drop === null) continue
    const nextSpeech = spans.find(([a]) => a > drop)?.[0] ?? drop + 1
    const end = Math.max(drop + 0.25, nextSpeech)
    if (t >= drop - 1 / 60 && t < end)
      g = Math.max(g, open * 1.15 - (open * 1.15 - g) * smoothstep((t - (end - 0.2)) / 0.2))
  }
  // Let the final hit and its ring-out sound.
  if (t >= timeline.finalHit - 0.05) g = Math.max(g, music.gainFinal)
  // Fade in from silence at the very start, and out at the very end.
  g *= smoothstep(t / 0.4) * smoothstep((timeline.duration - t) / 0.5)
  return g
}

export const Soundtrack: React.FC<{ stem?: 'voice' | 'music' }> = ({ stem }) => {
  const { timeline, spec } = useTimeline()
  const spans = speechSpans(timeline.captions)
  const fade = spec.music.joinFade
  const last = timeline.music.length - 1
  return (
    <>
      {stem !== 'voice' &&
        timeline.music.map((seg, i) => {
          // Neighbouring segments overlap by `joinFade` around each join, crossfaded at equal power.
          const s = seg.start - (i > 0 ? fade / 2 : 0)
          const e = seg.end + (i < last ? fade / 2 : 0)
          const from = f(s)
          const len = Math.max(1, f(e) - from)
          return (
            <Sequence key={`m${i}`} from={from} durationInFrames={len} layout="none">
              <Audio
                src={staticFile(paths(spec.id).music)}
                trimBefore={f(s + seg.trackOffset)}
                volume={(local) => {
                  const t = (from + local) / FPS
                  const fadeIn = i === 0 ? 1 : Math.sin((Math.PI / 2) * Math.min(1, Math.max(0, (t - s) / fade)))
                  const fadeOut =
                    i === last ? 1 : Math.cos((Math.PI / 2) * Math.min(1, Math.max(0, (t - (e - fade)) / fade)))
                  return musicGain(spec.music, timeline, spans, t) * fadeIn * fadeOut
                }}
              />
            </Sequence>
          )
        })}
      {stem !== 'music' &&
        timeline.voice.map((clip, i) => (
          <Sequence key={`v${i}`} from={f(clip.start)} durationInFrames={f(clip.to - clip.from) + 2} layout="none">
            <Audio
              src={staticFile(clip.file)}
              trimBefore={f(clip.from)}
              durationInFrames={f(clip.to - clip.from) + 2}
            />
          </Sequence>
        ))}
    </>
  )
}
