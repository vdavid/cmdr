import { AbsoluteFill, Img, staticFile } from 'remotion'
import { phraseIndexAt, useTimeline } from '../context.ts'
import { COLORS, FONT, MONO, ramp } from '../lib/motion.ts'
import type { SectionPlan } from '../timeline.ts'

// The presenter stands on the left; the right side answers each line with a visual, so it complements the
// captions instead of repeating them: a name, a 25-year counter, Windows → macOS, a search that finds nothing,
// and finally the logo.
export const Intro: React.FC<{ t: number; section: SectionPlan }> = ({ t, section }) => {
  const { timeline } = useTimeline()
  const caps = timeline.captions.filter((c) => c.sectionId === section.id)
  const idx = phraseIndexAt(timeline, section.id, t)
  const panels = [Name, Years, ToMac, NothingLikeIt, BuiltIt]
  return (
    <AbsoluteFill>
      {panels.map((Panel, i) => {
        if (!caps[i]) return null
        const start = caps[i].start - 0.12
        const next = caps[i + 1] ? caps[i + 1].start - 0.12 : section.end + 1
        if (t < start - 0.01 || t > next + 0.35) return null
        const enter = ramp(t, start, start + 0.45)
        const leave = i === idx ? 0 : ramp(t, next, next + 0.3)
        return (
          <div
            key={i}
            style={{
              position: 'absolute',
              left: 900,
              right: 80,
              top: 0,
              bottom: 120,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              opacity: enter * (1 - leave),
              transform: `translateY(${(1 - enter) * 60 - leave * 60}px)`,
              filter: `blur(${(1 - enter) * 10 + leave * 8}px)`,
            }}
          >
            <Panel t={t - start} />
          </div>
        )
      })}
    </AbsoluteFill>
  )
}

type PanelProps = { t: number }

const Big: React.FC<{ children: React.ReactNode; size?: number; color?: string }> = ({
  children,
  size = 150,
  color = COLORS.text,
}) => (
  <div
    style={{ fontFamily: FONT, fontWeight: 800, fontSize: size, letterSpacing: -size * 0.035, lineHeight: 1, color }}
  >
    {children}
  </div>
)
const Small: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={{ fontFamily: FONT, fontWeight: 500, fontSize: 44, color: COLORS.textDim, letterSpacing: -0.5 }}>
    {children}
  </div>
)

const Name: React.FC<PanelProps> = () => (
  <div style={{ textAlign: 'left' }}>
    <Small>Hi, I'm</Small>
    <Big size={190}>
      David<span style={{ color: COLORS.mustard }}>.</span>
    </Big>
  </div>
)

const Years: React.FC<PanelProps> = ({ t }) => {
  // A fixed number that pops in (a count-up would show other numbers than the 25 the voice says).
  const pop = ramp(t, 0.1, 0.45)
  const overshoot = 1 + Math.sin(pop * Math.PI) * 0.08
  return (
    <div style={{ textAlign: 'left' }}>
      <Big size={240} color={COLORS.mustard}>
        <span
          style={{
            display: 'inline-block',
            transform: `scale(${(0.6 + pop * 0.4) * overshoot})`,
            transformOrigin: '50% 80%',
          }}
        >
          25
        </span>
        <span style={{ fontSize: 120, color: COLORS.text, letterSpacing: -3 }}> years</span>
      </Big>
      <Small>of Total Commander.</Small>
    </div>
  )
}

const ToMac: React.FC<PanelProps> = ({ t }) => {
  const strike = ramp(t, 0.2, 0.55)
  const mac = ramp(t, 0.45, 0.9)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={{ position: 'relative', alignSelf: 'flex-start' }}>
        <Big size={120} color={COLORS.textDim}>
          Windows
        </Big>
        <div
          style={{
            position: 'absolute',
            left: -8,
            top: '54%',
            height: 10,
            width: `${strike * 104}%`,
            background: COLORS.mustard,
            borderRadius: 5,
          }}
        />
      </div>
      <div style={{ opacity: mac, transform: `translateX(${(1 - mac) * 80}px)` }}>
        <Big size={150}>macOS</Big>
      </div>
    </div>
  )
}

const NothingLikeIt: React.FC<PanelProps> = ({ t }) => {
  const query = 'two-pane file manager, keyboard-first'
  const typed = query.slice(0, Math.round(query.length * ramp(t, 0.05, 0.75, (x) => x)))
  const result = ramp(t, 0.85, 1.1)
  return (
    <div style={{ width: 820 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 22,
          padding: '26px 32px',
          background: COLORS.elevated,
          border: `2px solid ${COLORS.border}`,
          borderRadius: 22,
          fontFamily: MONO,
          fontSize: 36,
          color: COLORS.text,
        }}
      >
        <svg
          width="38"
          height="38"
          viewBox="0 0 24 24"
          fill="none"
          stroke={COLORS.textDim}
          strokeWidth="2.5"
          strokeLinecap="round"
        >
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <span>
          {typed}
          <span style={{ opacity: Math.floor(t * 3) % 2 === 0 ? 1 : 0, color: COLORS.mustard }}>▍</span>
        </span>
      </div>
      <div
        style={{
          marginTop: 26,
          marginLeft: 8,
          fontFamily: FONT,
          fontWeight: 700,
          fontSize: 54,
          color: '#ff4d6d',
          opacity: result,
          transform: `translateY(${(1 - result) * 20}px)`,
        }}
      >
        0 results. Nothing like it.
      </div>
    </div>
  )
}

const BuiltIt: React.FC<PanelProps> = ({ t }) => {
  const pop = ramp(t, 0.35, 0.8)
  const overshoot = 1 + Math.sin(Math.min(1, pop) * Math.PI) * 0.12
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 40 }}>
      <Img
        src={staticFile('brand/cmdr.svg')}
        style={{
          width: 260,
          height: 260,
          transform: `scale(${(0.4 + pop * 0.6) * overshoot}) rotate(${(1 - pop) * -14}deg)`,
          opacity: pop,
          filter: `drop-shadow(0 0 ${50 * pop}px rgba(255,194,6,0.45))`,
        }}
      />
      <div>
        <Small>So I built</Small>
        <Big size={170}>
          Cmdr<span style={{ color: COLORS.mustard }}>.</span>
        </Big>
      </div>
    </div>
  )
}
