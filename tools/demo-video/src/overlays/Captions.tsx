import { useT, useTimeline } from '../context.ts'
import { COLORS, FONT, ramp } from '../lib/motion.ts'

// Burned-in captions for muted viewing. One phrase at a time; words light up as they're spoken (timed by their share
// of the phrase's length, which tracks speech closely enough at this size).
export const Captions: React.FC = () => {
  const t = useT()
  const { timeline } = useTimeline()
  if (t >= timeline.finalHit - 0.05) return null
  const caps = timeline.captions
  const i = caps.findIndex(
    (c, k) => t >= c.start - 0.1 && t < Math.min(c.end + 0.45, (caps[k + 1]?.start ?? Infinity) - 0.1),
  )
  if (i < 0) return null
  const c = caps[i]
  const enter = ramp(t, c.start - 0.1, c.start + 0.08)
  const words = c.show.split(' ')
  const total = c.show.length
  let acc = 0
  return (
    <div style={{ position: 'absolute', left: 0, right: 0, bottom: 64, display: 'flex', justifyContent: 'center' }}>
      <div
        style={{
          maxWidth: 1240,
          padding: '14px 30px 16px',
          borderRadius: 18,
          background: 'rgba(10,10,11,0.72)',
          border: `1px solid ${COLORS.border}`,
          boxShadow: '0 12px 40px rgba(0,0,0,0.4)',
          fontFamily: FONT,
          fontWeight: 600,
          fontSize: 48,
          lineHeight: 1.25,
          letterSpacing: -0.6,
          textAlign: 'center',
          opacity: enter,
          transform: `translateY(${(1 - enter) * 14}px)`,
        }}
      >
        {words.map((w, k) => {
          const at = c.start + (acc / total) * (c.end - c.start)
          acc += w.length + 1
          const lit = ramp(t, at - 0.04, at + 0.1)
          return (
            <span key={k} style={{ color: `rgba(250,250,250,${0.38 + 0.62 * lit})` }}>
              {w}
              {k < words.length - 1 ? ' ' : ''}
            </span>
          )
        })}
      </div>
    </div>
  )
}
