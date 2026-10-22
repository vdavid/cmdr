import { AbsoluteFill, Img, staticFile } from 'remotion'
import { COLORS, FONT, ramp } from '../lib/motion.ts'

// The opening: the logo blooms out of the dark, a light glint crosses it, the wordmark slides out from behind it,
// then the whole thing pushes past the camera into the intro.
export const LogoSting: React.FC<{ t: number; end: number }> = ({ t, end }) => {
  const appear = ramp(t, 0.05, 0.75)
  const word = ramp(t, 0.45, 0.95)
  const exit = ramp(t, end - 0.4, end + 0.15)
  const glint = ramp(t, 0.55, 1.15)
  const logo = staticFile('brand/cmdr.svg')
  return (
    <AbsoluteFill
      style={{
        background: COLORS.bg,
        opacity: 1 - exit,
        alignItems: 'center',
        justifyContent: 'center',
        transform: `scale(${1 + exit * 1.8})`,
        filter: `blur(${exit * 14}px)`,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 36 * word }}>
        <div
          style={{
            position: 'relative',
            width: 300,
            height: 300,
            transform: `scale(${0.72 + appear * 0.28})`,
            opacity: appear,
            filter: `blur(${(1 - appear) * 18}px) drop-shadow(0 0 ${40 + 30 * (1 - glint)}px rgba(255,194,6,${0.35 * appear}))`,
          }}
        >
          <Img src={logo} style={{ width: 300, height: 300 }} />
          {/* The glint: a bright band masked to the logo's own silhouette. */}
          <div
            style={{
              position: 'absolute',
              inset: 0,
              WebkitMaskImage: `url(${logo})`,
              WebkitMaskSize: '100% 100%',
              background: `linear-gradient(105deg, transparent ${glint * 140 - 40}%, rgba(255,255,255,0.7) ${glint * 140 - 25}%, transparent ${glint * 140 - 10}%)`,
              mixBlendMode: 'screen',
            }}
          />
        </div>
        <div style={{ overflow: 'hidden', width: 520 * word }}>
          <div
            style={{
              fontFamily: FONT,
              fontWeight: 800,
              fontSize: 190,
              letterSpacing: -6,
              color: COLORS.text,
              transform: `translateX(${(1 - word) * -200}px)`,
              opacity: word,
              lineHeight: 1,
            }}
          >
            Cmdr
          </div>
        </div>
      </div>
    </AbsoluteFill>
  )
}
