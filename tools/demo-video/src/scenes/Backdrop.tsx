import { AbsoluteFill } from 'remotion'
import { useT, useTimeline } from '../context.ts'
import { COLORS, hits, inRanges } from '../lib/motion.ts'

// Near-black with a slow haze drifting behind everything: the logo's blue, violet, and pink for the first video,
// amber, rose, and teal for the warmer second one. On the music's pulses the haze breathes a little brighter.
export const Backdrop: React.FC = () => {
  const t = useT()
  const { timeline, spec } = useTimeline()
  const pulse = inRanges(t, timeline.highEnergy) ? hits(t, timeline.pulses, 0.18) : 0
  const a = 0.22 + pulse * 0.1
  const [c1, c2, c3, c4] = spec.theme.haze
  const blob = (x: number, y: number, r: number, color: string, alpha: number): string =>
    `radial-gradient(circle ${r}px at ${x}% ${y}%, ${color}${Math.round(Math.min(1, alpha) * 255)
      .toString(16)
      .padStart(2, '0')} 0%, transparent 70%)`
  return (
    <AbsoluteFill
      style={{
        background: [
          blob(18 + Math.sin(t * 0.11) * 8, 22 + Math.cos(t * 0.09) * 6, 900, c1, a),
          blob(85 + Math.cos(t * 0.07) * 6, 80 + Math.sin(t * 0.13) * 6, 950, c2, a * 0.8),
          blob(70 + Math.sin(t * 0.05) * 10, 10 + Math.sin(t * 0.08) * 5, 700, c3, a * 0.45),
          blob(25 + Math.cos(t * 0.06) * 8, 95, 800, c4, a * 0.35),
          COLORS.bg,
        ].join(', '),
      }}
    />
  )
}
