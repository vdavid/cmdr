import { AbsoluteFill, Easing, Freeze, Img, OffthreadVideo, Sequence, staticFile } from 'remotion'
import { useTimeline } from '../context.ts'
import { COLORS, FONT, MONO, hits, inRanges, ramp } from '../lib/motion.ts'
import { type Framing, revealAt, shotFraming } from '../lib/framing.ts'
import { f, type SectionPlan, type ShotPlan } from '../timeline.ts'

// A demo beat: a kinetic title card slams in on the cut, then hands over to the app window. The window never moves on
// the beat: its Cmdr-gold glow swells gently instead. Punch-ins are slow and land on the synced moment, framed by
// `lib/framing.ts` so the window always stays whole and on-canvas. The footage is the real recording when the manifest
// has it, else a still.
export const Beat: React.FC<{ t: number; section: SectionPlan }> = ({ t, section }) => {
  const { timeline, spec } = useTimeline()
  const { theme } = spec
  const local = t - section.start
  const cardOut = section.footageStart - section.start
  const energy = inRanges(t, timeline.highEnergy)
  const pulse = energy ? hits(t, timeline.pulses, 0.14) : 0
  // The glow decays slower than the haze, so it swells rather than flickers.
  const glow = energy ? hits(t, timeline.pulses, 0.3) * theme.glowPulse : 0
  const reveal = revealAt(section, t)
  const total = timeline.sections.filter((s) => s.kind === 'beat').length
  const shot = section.shots.find((s) => t >= s.start && t < s.end)
  const framing = shot ? shotFraming(shot, section, theme.transition, t) : null
  // `zoom` settles the window from a little larger; `slide` slides it in as the title leaves left.
  const e = framing?.enter ?? { scale: 1, x: 0 }
  const enter = {
    transform: `translateX(${e.x}px) scale(${e.scale})`,
    filter: `blur(${(1 - reveal) * (theme.transition === 'zoom' ? 12 : 6)}px)`,
  }
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ opacity: reveal, ...enter }}>
        <Glow t={t} pulse={pulse} colors={spec.theme.haze} />
        {shot && framing && <Shot key={section.shots.indexOf(shot)} shot={shot} framing={framing} glow={glow} />}
      </AbsoluteFill>
      <SectionTag section={section} index={section.index} total={total} t={local} cardOut={cardOut} />
      {local < cardOut + 0.3 &&
        (theme.titleStyle === 'slam' ? (
          <TitleCard text={section.title ?? ''} t={local} out={cardOut} />
        ) : (
          <GrooveCard text={section.title ?? ''} t={local} out={cardOut} beat={60 / spec.music.bpm} />
        ))}
    </AbsoluteFill>
  )
}

const Glow: React.FC<{ t: number; pulse: number; colors: [string, string, string, string] }> = ({
  t,
  pulse,
  colors,
}) => (
  <div
    style={{
      position: 'absolute',
      left: 260,
      right: 260,
      top: 170,
      bottom: 250,
      borderRadius: 60,
      background: `linear-gradient(${120 + Math.sin(t * 0.3) * 20}deg, ${colors[0]}, ${colors[1]} 45%, ${colors[3]})`,
      filter: 'blur(110px)',
      opacity: 0.32 + pulse * 0.12,
    }}
  />
)

/**
 * The app window's frame: a hairline gold edge, a soft drop shadow, and a Cmdr-gold (`COLORS.mustard`) glow whose
 * spread and strength swell with `glow` (0–1). It clips its content, so a punch-in stays inside it.
 */
export const windowChrome = (glow: number): React.CSSProperties => ({
  boxSizing: 'border-box',
  borderRadius: 14,
  overflow: 'hidden',
  border: `1px solid rgba(255,194,6,${0.3 + 0.2 * glow})`,
  boxShadow: [
    '0 40px 120px rgba(0,0,0,0.6)',
    `0 0 ${50 + 40 * glow}px ${4 + 10 * glow}px rgba(255,194,6,${0.22 + 0.2 * glow})`,
  ].join(', '),
})

const Shot: React.FC<{ shot: ShotPlan; framing: Framing; glow: number }> = ({ shot, framing, glow }) => {
  const { card, zoom, x, y } = framing
  const content: React.CSSProperties = {
    width: '100%',
    height: '100%',
    transform: `translate(${x}px, ${y}px) scale(${zoom})`,
    transformOrigin: '50% 50%',
  }
  return (
    <div style={{ position: 'absolute', ...card, ...windowChrome(glow) }}>
      {shot.footage ? (
        <div style={content}>
          <FootageClip shot={shot} />
        </div>
      ) : (
        <Img src={staticFile(`brand/screenshots/${shot.still}`)} style={content} />
      )}
    </div>
  )
}

/**
 * Plays a shot's footage in three parts: a hold on the first usable frame while the clip "catches up" to its synced
 * moment, the clip itself, and a hold on the last frame if the shot outlasts the recording.
 */
export const FootageClip: React.FC<{ shot: ShotPlan }> = ({ shot }) => {
  const footage = shot.footage
  if (!footage) return null
  const src = staticFile(footage.file)
  const start = f(shot.start)
  const end = f(shot.end)
  const holdIn = Math.max(0, f(footage.usableStart) - f(footage.from))
  const playFrom = Math.max(f(footage.from), f(footage.usableStart))
  const lastFrame = f(footage.duration) - 1
  const playLen = Math.max(0, Math.min(end - start - holdIn, lastFrame - playFrom))
  const holdOut = end - start - holdIn - playLen
  const fill = { width: '100%', height: '100%' }
  return (
    <>
      {holdIn > 0 && (
        <Sequence from={start} durationInFrames={holdIn} layout="none">
          <Freeze frame={playFrom}>
            <OffthreadVideo src={src} muted style={fill} />
          </Freeze>
        </Sequence>
      )}
      {playLen > 0 && (
        <Sequence from={start + holdIn} durationInFrames={playLen} layout="none">
          <OffthreadVideo src={src} trimBefore={playFrom} muted style={fill} />
        </Sequence>
      )}
      {holdOut > 0 && (
        <Sequence from={start + holdIn + playLen} durationInFrames={holdOut} layout="none">
          <Freeze frame={lastFrame}>
            <OffthreadVideo src={src} muted style={fill} />
          </Freeze>
        </Sequence>
      )}
    </>
  )
}

// The warmer title: words step in on eighth notes, dropping with a little bounce, then a mustard marker swipes behind
// the last word, like someone highlighting it by hand. It leaves by sliding left as the footage pushes in.
const GrooveCard: React.FC<{ text: string; t: number; out: number; beat: number }> = ({ text, t, out, beat }) => {
  const words = text.split(' ')
  const step = Math.min(beat / 2, 0.55 / Math.max(1, words.length - 1))
  const exit = ramp(t, out - 0.3, out + 0.2, Easing.bezier(0.7, 0, 0.84, 0))
  const marker = ramp(t, step * (words.length - 1) + 0.18, step * (words.length - 1) + 0.42)
  return (
    <AbsoluteFill
      style={{
        alignItems: 'center',
        justifyContent: 'center',
        opacity: 1 - exit,
        transform: `translateX(${exit * -320}px)`,
      }}
    >
      <div
        style={{
          display: 'flex',
          gap: '0 34px',
          flexWrap: 'wrap',
          justifyContent: 'center',
          maxWidth: 1500,
          fontFamily: FONT,
          fontWeight: 750,
          fontSize: 132,
          letterSpacing: -4,
          lineHeight: 1.12,
          color: COLORS.text,
        }}
      >
        {words.map((w, i) => {
          const a = ramp(t, i * step, i * step + 0.34, Easing.bezier(0.3, 1.6, 0.5, 1))
          const last = i === words.length - 1
          return (
            <span
              key={i}
              style={{
                position: 'relative',
                display: 'inline-block',
                transform: `translateY(${(1 - a) * -70}px) rotate(${(1 - a) * (i % 2 ? 4 : -4)}deg)`,
                opacity: Math.min(1, a * 1.6),
              }}
            >
              {last && (
                <span
                  style={{
                    position: 'absolute',
                    left: -14,
                    right: -14,
                    top: '18%',
                    bottom: '6%',
                    background: COLORS.mustard,
                    borderRadius: 14,
                    transform: `skewX(-8deg) scaleX(${marker})`,
                    transformOrigin: '0% 50%',
                  }}
                />
              )}
              <span style={{ position: 'relative', color: last && marker > 0.5 ? COLORS.bg : undefined }}>{w}</span>
            </span>
          )
        })}
      </div>
    </AbsoluteFill>
  )
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
  // It names the beat while the shot settles, then gets out of the footage's way.
  const show = ramp(t, cardOut, cardOut + 0.4) * (1 - ramp(t, cardOut + 3.2, cardOut + 3.8))
  return (
    <div
      style={{
        position: 'absolute',
        left: 52,
        top: 28,
        display: 'flex',
        alignItems: 'center',
        gap: 16,
        padding: '10px 20px',
        borderRadius: 999,
        // A frosted pill: punch-ins push busy app rows behind it, so it blurs and darkens whatever is there. It wipes
        // in and out instead of fading: a half-transparent backing let the rows show through the text mid-fade.
        background: 'rgba(10,10,11,0.88)',
        backdropFilter: 'blur(18px) saturate(140%)',
        WebkitBackdropFilter: 'blur(18px) saturate(140%)',
        border: '1px solid rgba(255,255,255,0.10)',
        clipPath: `inset(0 ${(1 - show) * 100}% 0 0 round 999px)`,
        visibility: show > 0.001 ? 'visible' : 'hidden',
        fontFamily: MONO,
        fontSize: 24,
        color: COLORS.textDim,
        letterSpacing: 1,
      }}
    >
      <span>
        <span style={{ color: COLORS.mustard }}>{String(index).padStart(2, '0')}</span>
        <span style={{ opacity: 0.5 }}> / {String(total).padStart(2, '0')}</span>
      </span>
      <span style={{ color: COLORS.mustard, opacity: 0.8 }}>·</span>
      <span>{(section.title ?? '').replace(/[.?!]$/, '')}</span>
    </div>
  )
}
