import { AbsoluteFill, random } from 'remotion'
import { FPS } from '../config.ts'
import { useT, useTimeline } from '../context.ts'
import { hit, hits } from '../lib/motion.ts'

// Finishing layers: a light leak that sweeps through on every cut, flashes on the drops, a vignette, and film grain.
// Subtle on purpose: they should be felt more than seen.

const GRAIN = `url("data:image/svg+xml;utf8,${encodeURIComponent(
  `<svg xmlns='http://www.w3.org/2000/svg' width='240' height='240'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 0.5  0 0 0 0 0.5  0 0 0 0 0.5  0 0 0 1.4 -0.2'/></filter><rect width='100%' height='100%' filter='url(#n)'/></svg>`,
)}")`

export const Effects: React.FC = () => {
  const t = useT()
  const { timeline, spec } = useTimeline()
  const LEAKS = spec.theme.leaks
  const frame = Math.round(t * FPS)
  const cuts = timeline.sections.slice(1).map((s) => s.start)
  const leaks = cuts.map((at, i) => ({ at, i, d: t - at })).filter(({ d }) => d >= -0.05 && d < 1.1)
  // The drop-and-glitch theme flashes on the drops and cuts; the warm theme lets the light leaks carry the cuts.
  const flash = spec.theme.flashes
    ? Math.max(
        hit(t, timeline.drop1, 0.07) * 0.35,
        timeline.bigDrop === null ? 0 : hit(t, timeline.bigDrop, 0.08) * 0.5,
      )
    : 0
  const goldFlash = hit(t, timeline.finalHit, 0.1) * (spec.theme.flashes ? 0.45 : 0.25)
  const cutFlash = spec.theme.flashes ? hits(t, cuts, 0.05) * 0.12 : 0
  return (
    <AbsoluteFill style={{ pointerEvents: 'none' }}>
      {leaks.map(({ at, i, d }) => {
        const [a, b] = LEAKS[i % LEAKS.length]
        const p = Math.max(0, d) / 1.1
        const strength = Math.sin(Math.min(1, p) * Math.PI) * 0.9
        const x = -20 + p * 140
        return (
          <AbsoluteFill
            key={at}
            style={{
              mixBlendMode: 'screen',
              opacity: strength,
              background: `radial-gradient(ellipse 900px 1300px at ${x}% ${30 + ((i * 13) % 40)}%, ${a}, transparent 60%), radial-gradient(ellipse 600px 900px at ${x - 18}% 80%, ${b}, transparent 65%)`,
              filter: 'blur(30px)',
            }}
          />
        )
      })}
      <AbsoluteFill style={{ background: '#fff', opacity: flash + cutFlash, mixBlendMode: 'overlay' }} />
      <AbsoluteFill style={{ background: '#ffc206', opacity: goldFlash, mixBlendMode: 'screen' }} />
      <AbsoluteFill
        style={{ background: 'radial-gradient(ellipse at 50% 45%, transparent 55%, rgba(0,0,0,0.55) 100%)' }}
      />
      <AbsoluteFill
        style={{
          backgroundImage: GRAIN,
          backgroundPosition: `${Math.floor(random(`gx${frame}`) * 240)}px ${Math.floor(random(`gy${frame}`) * 240)}px`,
          opacity: 0.075,
          mixBlendMode: 'overlay',
        }}
      />
    </AbsoluteFill>
  )
}
