import { Audio, Sequence, staticFile } from 'remotion'
import { FPS, MUSIC } from '../config.ts'
import { useTimeline } from '../context.ts'
import { f, type Caption, type Timeline } from '../timeline.ts'

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
export function musicGain(timeline: Timeline, spans: [number, number][], t: number): number {
  let d = Infinity
  for (const [a, b] of spans) d = Math.min(d, t < a ? a - t : t > b ? t - b : 0)
  const open = MUSIC.gainOpen
  let g = MUSIC.gainUnderVoice + (open - MUSIC.gainUnderVoice) * smoothstep(d / 0.35)
  // The intro sits a little lower so the sting and the first words feel intimate, then the first drop opens up.
  if (t < timeline.drop1 - 0.1) g *= 0.75
  // Punch windows: each drop hits fully open, however close the surrounding lines are, then eases back under the
  // next line over a short ramp.
  for (const drop of [timeline.drop1, timeline.bigDrop]) {
    const nextSpeech = spans.find(([a]) => a > drop)?.[0] ?? drop + 1
    const end = Math.max(drop + 0.25, nextSpeech)
    if (t >= drop - 1 / 60 && t < end)
      g = Math.max(g, open * 1.15 - (open * 1.15 - g) * smoothstep((t - (end - 0.2)) / 0.2))
  }
  // Let the final hit and its decay ring out.
  if (t >= timeline.finalHit - 0.05) g = Math.max(g, MUSIC.gainFinal)
  // Fade in from silence at the very start, and out at the very end.
  g *= smoothstep(t / 0.4) * smoothstep((timeline.duration - t) / 0.5)
  return g
}

export const Soundtrack: React.FC<{ stem?: 'voice' | 'music' }> = ({ stem }) => {
  const { timeline } = useTimeline()
  const spans = speechSpans(timeline.captions)
  return (
    <>
      {stem !== 'voice' &&
        timeline.music.map((seg, i) => {
          const from = f(seg.start)
          const len = f(seg.end) - from
          return (
            <Sequence key={`m${i}`} from={from} durationInFrames={len} layout="none">
              <Audio
                src={staticFile(MUSIC.file)}
                trimBefore={f(seg.start + seg.trackOffset)}
                volume={(local) => {
                  // A one-frame fade at internal joins keeps the bar-aligned jumps click-free.
                  const edge = Math.min(
                    i === 0 ? 1 : (local + 1) / 2,
                    i === timeline.music.length - 1 ? 1 : (len - local) / 2,
                  )
                  return musicGain(timeline, spans, (from + local) / FPS) * Math.min(1, edge)
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
