import { AbsoluteFill, Easing, Img, staticFile } from 'remotion'
import { useTimeline } from '../context.ts'
import { COLORS, FONT, ramp } from '../lib/motion.ts'
import type { SectionPlan } from '../timeline.ts'

// The second video's intro, answering the presenter on the right: "everywhere." lands big; then each place pops in
// as it's named (Mac, NAS, phone, cloud, backups), arranged in a ring; then the Cmdr logo drops into the middle and
// files start flowing along lines to and from every place.

const CENTER = { x: 1360, y: 470 }
const RING = { rx: 330, ry: 245 }

type Place = { label: string; word: string; icon: React.ReactNode }
const stroke = {
  fill: 'none',
  stroke: COLORS.mustard,
  strokeWidth: 1.8,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
}
const PLACES: Place[] = [
  {
    label: 'Mac',
    word: 'Mac',
    icon: (
      <>
        <rect x="4" y="5" width="16" height="11" rx="1.5" {...stroke} />
        <path d="M2 19h20" {...stroke} />
      </>
    ),
  },
  {
    label: 'NAS',
    word: 'NAS',
    icon: (
      <>
        <rect x="4" y="3.5" width="16" height="7" rx="1.5" {...stroke} />
        <rect x="4" y="13.5" width="16" height="7" rx="1.5" {...stroke} />
        <path d="M7.5 7h.01M7.5 17h.01" {...stroke} strokeWidth={2.6} />
      </>
    ),
  },
  {
    label: 'Phone',
    word: 'phone',
    icon: (
      <>
        <rect x="7" y="2.5" width="10" height="19" rx="2.2" {...stroke} />
        <path d="M11 18h2" {...stroke} />
      </>
    ),
  },
  {
    label: 'Cloud',
    word: 'cloud',
    icon: <path d="M7 18.5h10a4 4 0 0 0 .6-7.95A6 6 0 0 0 6.2 11.2 3.7 3.7 0 0 0 7 18.5z" {...stroke} />,
  },
  {
    label: 'Backups',
    word: 'backups',
    icon: (
      <>
        <path d="M4 12a8 8 0 1 0 2.4-5.7L4 8.6" {...stroke} />
        <path d="M4 4v4.6h4.6M12 8v4.3l3 1.8" {...stroke} />
      </>
    ),
  },
]

const pos = (i: number, spread: number): { x: number; y: number } => {
  const a = (-90 + i * 72) * (Math.PI / 180)
  return { x: CENTER.x + Math.cos(a) * RING.rx * spread, y: CENTER.y + Math.sin(a) * RING.ry * spread }
}

export const IntroEverywhere: React.FC<{ t: number; section: SectionPlan }> = ({ t, section }) => {
  const { timeline } = useTimeline()
  const caps = timeline.captions.filter((c) => c.sectionId === section.id)
  if (caps.length < 3) return null
  const [title, list, moves] = caps
  // When each place is named: its word's share of the list phrase's length.
  const wordAt = (word: string): number => {
    const i = list.show.indexOf(word)
    return list.start + (Math.max(0, i) / list.show.length) * (list.end - list.start) - 0.08
  }
  const titleIn = ramp(t, title.start - 0.1, title.start + 0.4)
  const titleOut = ramp(t, list.start - 0.25, list.start + 0.15)
  const hub = ramp(t, moves.start, moves.start + 0.5, Easing.bezier(0.2, 1.5, 0.4, 1))
  const spread = 1 + hub * 0.08

  return (
    <AbsoluteFill>
      {/* "everywhere." */}
      <div
        style={{
          position: 'absolute',
          left: 900,
          right: 80,
          top: 330,
          textAlign: 'center',
          opacity: titleIn * (1 - titleOut),
          transform: `translateY(${(1 - titleIn) * 40 - titleOut * 50}px) scale(${1 - titleOut * 0.1})`,
          filter: `blur(${titleOut * 10}px)`,
        }}
      >
        <div style={{ fontFamily: FONT, fontWeight: 500, fontSize: 48, color: COLORS.textDim }}>Your files live</div>
        <div
          style={{
            fontFamily: FONT,
            fontWeight: 800,
            fontSize: 170,
            letterSpacing: -6,
            color: COLORS.text,
            lineHeight: 1.05,
          }}
        >
          everywhere<span style={{ color: COLORS.mustard }}>.</span>
        </div>
      </div>

      {/* Lines from the hub to every place, with files flowing both ways. */}
      <svg width={1920} height={1080} style={{ position: 'absolute', inset: 0, overflow: 'visible' }}>
        {PLACES.map((p, i) => {
          const end = pos(i, spread)
          const draw = ramp(t, moves.start + 0.15 + i * 0.07, moves.start + 0.6 + i * 0.07)
          if (draw <= 0) return null
          const x2 = CENTER.x + (end.x - CENTER.x) * draw
          const y2 = CENTER.y + (end.y - CENTER.y) * draw
          const dots = [0, 0.5].map((phase, k) => {
            const u = ((((t - moves.start) * 0.55 + phase + i * 0.17) % 1) + 1) % 1
            // Alternate directions per place: some files go out, some come back.
            const v = i % 2 === k % 2 ? u : 1 - u
            return {
              x: CENTER.x + (end.x - CENTER.x) * v,
              y: CENTER.y + (end.y - CENTER.y) * v,
              a: Math.sin(u * Math.PI),
            }
          })
          return (
            <g key={p.label}>
              <line
                x1={CENTER.x}
                y1={CENTER.y}
                x2={x2}
                y2={y2}
                stroke="rgba(255,194,6,0.35)"
                strokeWidth={2}
                strokeDasharray="2 10"
                strokeLinecap="round"
              />
              {draw >= 1 &&
                dots.map((d, k) => (
                  <circle key={k} cx={d.x} cy={d.y} r={6} fill={COLORS.mustard} opacity={d.a * 0.9} />
                ))}
            </g>
          )
        })}
      </svg>

      {/* The places. */}
      {PLACES.map((p, i) => {
        const at = wordAt(p.word)
        const pop = ramp(t, at, at + 0.4, Easing.bezier(0.2, 1.6, 0.4, 1))
        if (pop <= 0) return null
        const { x, y } = pos(i, spread)
        return (
          <div
            key={p.label}
            style={{
              position: 'absolute',
              left: x - 95,
              top: y - 62,
              width: 190,
              height: 124,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: 10,
              borderRadius: 26,
              background: 'rgba(24,24,27,0.88)',
              border: `1.5px solid ${COLORS.border}`,
              boxShadow: `0 16px 40px rgba(0,0,0,0.45), 0 0 ${30 * hub}px rgba(255,194,6,${0.18 * hub})`,
              transform: `scale(${0.5 + 0.5 * pop}) translateY(${Math.sin(t * 1.4 + i) * 4}px)`,
              opacity: Math.min(1, pop * 1.5),
            }}
          >
            <svg width={52} height={52} viewBox="0 0 24 24">
              {p.icon}
            </svg>
            <div style={{ fontFamily: FONT, fontWeight: 600, fontSize: 30, color: COLORS.text, letterSpacing: -0.4 }}>
              {p.label}
            </div>
          </div>
        )
      })}

      {/* The hub: Cmdr in the middle. */}
      {hub > 0 && (
        <Img
          src={staticFile('brand/cmdr.svg')}
          style={{
            position: 'absolute',
            left: CENTER.x - 105,
            top: CENTER.y - 105,
            width: 210,
            height: 210,
            transform: `scale(${0.3 + 0.7 * hub})`,
            opacity: Math.min(1, hub * 2),
            filter: `drop-shadow(0 0 ${50 * hub}px rgba(255,194,6,0.45))`,
          }}
        />
      )}
    </AbsoluteFill>
  )
}
