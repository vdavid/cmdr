import { Easing, interpolate } from 'remotion'
import { PRESENTER } from '../config.ts'
import { useT, useTimeline, voiceLevel } from '../context.ts'
import { hits, inRanges } from '../lib/motion.ts'
import { StickFigure } from './StickFigure.tsx'
import { Webcam } from './Webcam.tsx'

// The presenter slot: one layer for the whole video. It's full size at the intro and close, and a round "webcam
// bubble" in the corner during the demo beats, morphing between the two. What's inside is swappable: the stick
// figure today, David's webcam takes later (`PRESENTER` in config.ts).

type Frame = { bubble: number; cx: number; cy: number; visible: number }
const FULL_LEFT: Frame = { bubble: 0, cx: 470, cy: 560, visible: 1 }
const BUBBLE_RIGHT: Frame = { bubble: 1, cx: 1738, cy: 872, visible: 1 }
const BUBBLE_LEFT: Frame = { bubble: 1, cx: 180, cy: 900, visible: 1 }
const MORPH = 0.55

/** Full-size box and the bubble's diameter, in px. The figure's SVG is 400 × 600 units. */
export const FULL_W = 560
export const FULL_H = 820
export const BUBBLE_D = 268

export const Presenter: React.FC = () => {
  const t = useT()
  const { timeline, voice } = useTimeline()
  const intro = timeline.sections[0]
  const close = timeline.sections[timeline.sections.length - 1]
  const keys: [number, Frame][] = [
    [timeline.voice[0].start - 0.45, { ...FULL_LEFT, visible: 0 }],
    [timeline.voice[0].start - 0.45 + MORPH, FULL_LEFT],
    [intro.end - 0.05, FULL_LEFT],
    [intro.end - 0.05 + MORPH, BUBBLE_RIGHT],
    [close.start, BUBBLE_RIGHT],
    [close.start + MORPH, FULL_LEFT],
    [timeline.finalHit - 0.04, FULL_LEFT],
    [timeline.finalHit - 0.04 + MORPH, BUBBLE_LEFT],
  ]
  const frame = at(keys, t)
  if (frame.visible <= 0.001) return null

  const level = voiceLevel(timeline, voice, t)
  const beat = inRanges(t, timeline.highEnergy) ? hits(t, timeline.pulses, 0.16) : 0
  const b = frame.bubble
  const w = FULL_W + (BUBBLE_D - FULL_W) * b
  const h = FULL_H + (BUBBLE_D - FULL_H) * b
  // The bubble sits a hair below the title-card exit, and pops a little on each downbeat during the drops.
  const pop = 1 + beat * 0.03 * b
  return (
    <div
      style={{
        position: 'absolute',
        left: frame.cx - w / 2,
        top: frame.cy - h / 2,
        width: w,
        height: h,
        opacity: frame.visible,
        transform: `scale(${(0.9 + 0.1 * frame.visible) * pop})`,
        borderRadius: interpolate(b, [0, 0.6, 1], [28, 120, BUBBLE_D / 2]),
        overflow: 'hidden',
        background: `rgba(24,24,27,${b * 0.92})`,
        boxShadow: [
          `0 0 0 ${3 * b}px rgba(255,194,6,${b})`,
          `0 0 ${(30 + level * 40) * b}px rgba(255,194,6,${(0.25 + level * 0.35) * b})`,
          `0 24px 60px rgba(0,0,0,${0.55 * b})`,
        ].join(', '),
      }}
    >
      {PRESENTER.kind === 'webcam' ? (
        <Webcam />
      ) : (
        <StickFigure t={t} level={level} bubble={b} w={w} h={h} beat={beat} />
      )}
    </div>
  )
}

function at(keys: [number, Frame][], t: number): Frame {
  if (t <= keys[0][0]) return keys[0][1]
  for (let i = 1; i < keys.length; i++) {
    const [t1, b] = keys[i]
    if (t <= t1) {
      const [t0, a] = keys[i - 1]
      const p = Easing.bezier(0.65, 0, 0.35, 1)((t - t0) / Math.max(1e-6, t1 - t0))
      return {
        bubble: a.bubble + (b.bubble - a.bubble) * p,
        cx: a.cx + (b.cx - a.cx) * p,
        cy: a.cy + (b.cy - a.cy) * p,
        visible: a.visible + (b.visible - a.visible) * p,
      }
    }
  }
  return keys[keys.length - 1][1]
}
