import { AbsoluteFill, Easing, Img, OffthreadVideo, Sequence, staticFile } from 'remotion'
import { useTimeline } from '../context.ts'
import { COLORS, FONT, MONO, hit, ramp } from '../lib/motion.ts'
import { f, type SectionPlan } from '../timeline.ts'

// The close: the presenter is back full size with the app floating beside them; on the music's final hit the frame
// clears and the end card slams in (logo, name, URL), then holds through the decay.
export const Close: React.FC<{ t: number; section: SectionPlan }> = ({ t, section }) => {
  const { timeline, spec } = useTimeline()
  const local = t - section.start
  const hitT = timeline.finalHit
  const cleared = ramp(t, hitT - 0.06, hitT + 0.05)
  const shot = section.shots[0]
  const enter = ramp(local, 0, 0.8)
  return (
    <AbsoluteFill>
      {/* The app, floating in perspective on the right. */}
      <AbsoluteFill style={{ perspective: 1800, opacity: (1 - cleared) * enter }}>
        <div
          style={{
            position: 'absolute',
            left: 860,
            top: 150,
            width: 1180,
            height: 1180 * (1634 / 2508),
            transform: `rotateY(${-22 + local * 1.6}deg) rotateX(${5 - local * 0.4}deg) translateX(${(1 - enter) * 160}px)`,
            transformOrigin: '0% 50%',
            filter: `drop-shadow(0 0 80px rgba(139,92,246,0.35))`,
          }}
        >
          {shot?.footage ? (
            <Sequence from={f(shot.start)} durationInFrames={f(shot.end - shot.start)} layout="none">
              <OffthreadVideo
                src={staticFile(shot.footage.file)}
                trimBefore={f(shot.footage.from)}
                muted
                style={{ width: '100%', height: '100%', borderRadius: 18 }}
              />
            </Sequence>
          ) : (
            <Img
              src={staticFile(`brand/screenshots/${shot?.still ?? 'app-main-dark.webp'}`)}
              style={{ width: '100%' }}
            />
          )}
        </div>
      </AbsoluteFill>
      {t >= hitT - 0.06 && <EndCard t={t - hitT} tagline={spec.endTagline} credit={spec.music.credit} />}
    </AbsoluteFill>
  )
}

const EndCard: React.FC<{ t: number; tagline: string; credit: string }> = ({ t, tagline, credit }) => {
  const slam = ramp(t, 0, 0.32, Easing.bezier(0.2, 1.6, 0.4, 1))
  const shock = ramp(t, 0, 0.9)
  const word = ramp(t, 0.12, 0.5)
  const url = ramp(t, 0.3, 0.7)
  const small = ramp(t, 0.55, 1.0)
  const glow = 0.45 + hit(t, 0, 0.35) * 0.55
  return (
    <AbsoluteFill style={{ alignItems: 'center', justifyContent: 'center' }}>
      <div
        style={{
          position: 'absolute',
          width: 300,
          height: 300,
          top: 210,
          borderRadius: '50%',
          border: `3px solid rgba(255,194,6,${0.6 * (1 - shock)})`,
          transform: `scale(${1 + shock * 3.2})`,
        }}
      />
      <div
        style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', transform: `scale(${1 + t * 0.012})` }}
      >
        <Img
          src={staticFile('brand/cmdr.svg')}
          style={{
            width: 300,
            height: 300,
            transform: `scale(${1.5 - slam * 0.5})`,
            opacity: Math.min(1, slam * 3),
            filter: `drop-shadow(0 0 ${70 * glow}px rgba(255,194,6,${0.5 * glow}))`,
          }}
        />
        <div
          style={{
            marginTop: 14,
            fontFamily: FONT,
            fontWeight: 800,
            fontSize: 130,
            letterSpacing: -5,
            color: COLORS.text,
            opacity: word,
            transform: `translateY(${(1 - word) * 30}px)`,
            lineHeight: 1,
          }}
        >
          Cmdr
        </div>
        <div
          style={{
            marginTop: 34,
            padding: '18px 44px',
            borderRadius: 999,
            background: COLORS.mustard,
            color: COLORS.bg,
            fontFamily: FONT,
            fontWeight: 700,
            fontSize: 48,
            letterSpacing: -0.5,
            opacity: url,
            transform: `translateY(${(1 - url) * 24}px) scale(${0.92 + url * 0.08})`,
            boxShadow: `0 0 ${40 * url}px rgba(255,194,6,0.4)`,
          }}
        >
          getcmdr.com
        </div>
        <div
          style={{
            marginTop: 28,
            fontFamily: MONO,
            fontSize: 26,
            color: COLORS.textDim,
            opacity: small,
            letterSpacing: 1,
          }}
        >
          {tagline}
        </div>
      </div>
      {/* CC BY 3.0 needs visible attribution; the license link lives in CREDITS.md and the video description. */}
      <div
        style={{
          position: 'absolute',
          right: 56,
          bottom: 44,
          fontFamily: MONO,
          fontSize: 20,
          letterSpacing: 0.5,
          color: COLORS.textDim,
          opacity: small * 0.75,
        }}
      >
        {credit}
      </div>
    </AbsoluteFill>
  )
}
