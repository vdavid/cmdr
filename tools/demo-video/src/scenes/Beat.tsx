import { AbsoluteFill, Easing, Img, OffthreadVideo, Sequence, staticFile } from 'remotion'
import { useTimeline } from '../context.ts'
import { COLORS, FONT, MONO, hits, inRanges, ramp } from '../lib/motion.ts'
import { f, type SectionPlan, type ShotPlan } from '../timeline.ts'

// A demo beat: a kinetic title card slams in on the cut, then pushes through into the app footage, which drifts and
// punches in on the key UI moment. The footage is the real recording when the manifest has it, else a still.
export const Beat: React.FC<{ t: number; section: SectionPlan }> = ({ t, section }) => {
  const { timeline } = useTimeline()
  const local = t - section.start
  const cardOut = section.footageStart - section.start
  const pulse = inRanges(t, timeline.highEnergy) ? hits(t, timeline.downbeats, 0.14) : 0
  const reveal = ramp(local, cardOut - 0.3, cardOut + 0.25, Easing.bezier(0.16, 1, 0.3, 1))
  const total = timeline.sections.filter((s) => s.kind === 'beat').length
  return (
    <AbsoluteFill>
      <AbsoluteFill
        style={{
          opacity: reveal,
          transform: `scale(${1.35 - 0.35 * reveal + pulse * 0.012})`,
          filter: `blur(${(1 - reveal) * 12}px)`,
        }}
      >
        <Glow t={t} pulse={pulse} />
        {section.shots.map((shot, i) =>
          t >= shot.start && t < shot.end ? <Shot key={i} shot={shot} t={t} first={i === 0} /> : null,
        )}
      </AbsoluteFill>
      <SectionTag section={section} index={section.index} total={total} t={local} cardOut={cardOut} />
      {local < cardOut + 0.3 && <TitleCard text={section.title ?? ''} t={local} out={cardOut} />}
    </AbsoluteFill>
  )
}

const Glow: React.FC<{ t: number; pulse: number }> = ({ t, pulse }) => (
  <div
    style={{
      position: 'absolute',
      left: 260,
      right: 260,
      top: 170,
      bottom: 250,
      borderRadius: 60,
      background: `linear-gradient(${120 + Math.sin(t * 0.3) * 20}deg, ${COLORS.violet}, ${COLORS.blue} 45%, ${COLORS.mustard})`,
      filter: 'blur(110px)',
      opacity: 0.32 + pulse * 0.12,
    }}
  />
)

// Base footage box: centered a bit high, leaving the bottom band for captions.
const BOX = { cx: 960, cy: 478, w: 1440 }

const Shot: React.FC<{ shot: ShotPlan; t: number; first: boolean }> = ({ shot, t, first }) => {
  const len = shot.end - shot.start
  const local = t - shot.start
  const focus = shot.focus ?? { x: 0.5, y: 0.5, zoom: 1 }
  // The first shot drifts in slowly, then punches in on the key moment; later shots cut in already close.
  const drift = ramp(local, 0, len, (x) => x)
  const punchAt = first ? len * 0.45 : 0
  const punch = first ? ramp(local, punchAt, punchAt + 0.45, Easing.bezier(0.2, 1.4, 0.4, 1)) : 1
  const p = first ? 0.25 * drift + 0.75 * punch : 0.7 + 0.3 * drift
  const zoom = 1 + (focus.zoom - 1) * p
  const isVideo = shot.footage !== undefined
  const aspect = isVideo ? 9 / 16 : 1634 / 2508
  const h = BOX.w * aspect
  const dx = (0.5 - focus.x) * BOX.w * p
  const dy = (0.5 - focus.y) * h * p
  const style: React.CSSProperties = {
    position: 'absolute',
    left: BOX.cx - BOX.w / 2,
    top: BOX.cy - h / 2,
    width: BOX.w,
    height: h,
    transform: `scale(${zoom}) translate(${dx}px, ${dy}px)`,
    transformOrigin: '50% 50%',
  }
  if (shot.footage) {
    const from = f(shot.footage.from)
    const available = f(shot.footage.duration) - from
    return (
      <div
        style={{
          ...style,
          borderRadius: 18,
          overflow: 'hidden',
          border: `1px solid ${COLORS.border}`,
          boxShadow: '0 40px 120px rgba(0,0,0,0.6)',
        }}
      >
        <Sequence from={f(shot.start)} durationInFrames={Math.max(1, Math.min(f(len), available))} layout="none">
          <OffthreadVideo
            src={staticFile(shot.footage.file)}
            trimBefore={from}
            muted
            style={{ width: '100%', height: '100%' }}
          />
        </Sequence>
      </div>
    )
  }
  return <Img src={staticFile(`brand/screenshots/${shot.still}`)} style={style} />
}

const TitleCard: React.FC<{ text: string; t: number; out: number }> = ({ text, t, out }) => {
  const exit = ramp(t, out - 0.3, out + 0.2, Easing.bezier(0.7, 0, 0.84, 0))
  const letters = [...text]
  // The trailing period (or question mark) gets the mustard.
  const accent = /[.?!]$/.test(text) ? letters.length - 1 : -1
  return (
    <AbsoluteFill
      style={{
        alignItems: 'center',
        justifyContent: 'center',
        opacity: 1 - exit,
        transform: `scale(${1 + exit * 1.6})`,
        filter: `blur(${exit * 10}px)`,
      }}
    >
      {/* A big outlined echo behind, slowly drifting, gives the type depth without a template look. */}
      <div
        style={{
          position: 'absolute',
          fontFamily: FONT,
          fontWeight: 900,
          fontSize: 300,
          letterSpacing: -12,
          color: 'transparent',
          WebkitTextStroke: `2px rgba(255,194,6,${0.18 * (1 - ramp(t, 0, 1))})`,
          transform: `scale(${1.15 + t * 0.12}) translateY(10px)`,
          whiteSpace: 'nowrap',
        }}
      >
        {text}
      </div>
      <div
        style={{
          display: 'flex',
          fontFamily: FONT,
          fontWeight: 800,
          fontSize: 168,
          letterSpacing: -6,
          color: COLORS.text,
          whiteSpace: 'pre',
        }}
      >
        {letters.map((ch, i) => {
          const a = ramp(t, i * 0.022, i * 0.022 + 0.32, Easing.bezier(0.16, 1, 0.3, 1))
          return (
            <span
              key={i}
              style={{
                display: 'inline-block',
                transform: `translateY(${(1 - a) * 90}px) skewY(${(1 - a) * 8}deg)`,
                opacity: a,
                filter: `blur(${(1 - a) * 8}px)`,
                color: i === accent ? COLORS.mustard : undefined,
              }}
            >
              {ch}
            </span>
          )
        })}
      </div>
      <div
        style={{
          position: 'absolute',
          top: 680,
          height: 6,
          width: 360 * ramp(t, 0.12, 0.5),
          background: COLORS.mustard,
          borderRadius: 3,
          boxShadow: `0 0 24px ${COLORS.mustard}`,
        }}
      />
    </AbsoluteFill>
  )
}

const SectionTag: React.FC<{ section: SectionPlan; index: number; total: number; t: number; cardOut: number }> = ({
  section,
  index,
  total,
  t,
  cardOut,
}) => {
  const show = ramp(t, cardOut, cardOut + 0.4)
  return (
    <div
      style={{
        position: 'absolute',
        left: 52,
        top: 44,
        display: 'flex',
        alignItems: 'center',
        gap: 16,
        padding: '10px 18px',
        borderRadius: 12,
        background: 'rgba(10,10,11,0.78)',
        border: `1px solid ${COLORS.border}`,
        opacity: show * 0.9,
        transform: `translateX(${(1 - show) * -20}px)`,
        fontFamily: MONO,
        fontSize: 24,
        color: COLORS.textDim,
        letterSpacing: 1,
      }}
    >
      <span style={{ color: COLORS.mustard }}>{String(index).padStart(2, '0')}</span>
      <span style={{ width: 28, height: 2, background: COLORS.border }} />
      <span>{(section.title ?? '').replace(/[.?!]$/, '')}</span>
      <span style={{ opacity: 0.5 }}>/ {String(total).padStart(2, '0')}</span>
    </div>
  )
}
