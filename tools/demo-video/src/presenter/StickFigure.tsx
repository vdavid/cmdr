import { random } from 'remotion'
import { FPS } from '../config.ts'
import { phraseIndexAt, sectionAt, useTimeline } from '../context.ts'
import { COLORS, boil, clamp01 } from '../lib/motion.ts'

// "David" as a stick figure: round glasses in Cmdr mustard, a hair tuft, eyebrows that lift on emphasis, a mouth
// driven by the voice's loudness, and arm gestures cued per phrase. Lines "boil" (redrawn every few frames with a
// small wobble) like hand-drawn animation, which keeps it from looking like a vector template.

type Arm = [upper: number, bend: number]
type Pose = { l: Arm; r: Arm; tilt: number; hop: number }
type PoseName = 'idle' | 'wave' | 'point' | 'open' | 'shrug' | 'cheer' | 'thumb'

const POSES: Record<PoseName, Pose> = {
  idle: { l: [16, 12], r: [16, 12], tilt: 0, hop: 0 },
  wave: { l: [16, 12], r: [148, -40], tilt: -4, hop: 0 },
  point: { l: [16, 12], r: [86, 4], tilt: 5, hop: 0 },
  open: { l: [52, 28], r: [52, 28], tilt: 0, hop: 0 },
  shrug: { l: [58, 102], r: [58, 102], tilt: 9, hop: 0 },
  cheer: { l: [158, 8], r: [158, 8], tilt: 0, hop: 1 },
  thumb: { l: [16, 12], r: [34, 118], tilt: -3, hop: 0 },
}

/** One pose per caption phrase; after the final hit he waves goodbye. */
const GESTURES: Record<string, PoseName[]> = {
  intro: ['wave', 'point', 'open', 'shrug', 'cheer'],
  speed: ['idle', 'point', 'open', 'cheer', 'point'],
  trust: ['idle', 'open', 'point', 'shrug', 'wave', 'idle', 'thumb'],
  rename: ['open', 'idle', 'point'],
  reach: ['open', 'point', 'idle', 'open', 'idle', 'thumb'],
  ai: ['idle', 'shrug', 'idle', 'open', 'idle', 'thumb'],
  care: ['open', 'idle', 'idle', 'cheer'],
  close: ['cheer', 'open', 'point'],
}

const SHOULDER = { x: 200, y: 270 }
const HIP = { x: 200, y: 432 }
const UPPER = 92
const FORE = 84
const STROKE = COLORS.text

export const StickFigure: React.FC<{
  t: number
  level: number
  bubble: number
  w: number
  h: number
  beat: number
}> = ({ t, level, bubble, w, h, beat }) => {
  const { timeline } = useTimeline()
  const frame = Math.round(t * FPS)
  const section = sectionAt(timeline, t)
  const pose = currentPose(timeline, section.id, t)
  const open = clamp01((level - 0.06) / 0.75)

  const sway = Math.sin(t * 0.9) * 2.2
  const hop = pose.hop * Math.abs(Math.sin(t * 7)) * 14
  const bob = beat * 5 + Math.sin(t * 2.1) * 2 + open * 2
  const blink = blinking(t)
  const brow = clamp01((level - 0.55) * 2.5) * 7 + (pose.tilt > 6 ? 5 : 0)

  // viewBox morphs from the whole body (full size) to head and shoulders (bubble).
  const vb = [0 + 65 * bubble, 0 + 55 * bubble, 400 - 130 * bubble, 600 - 330 * bubble]

  const arm = (side: -1 | 1, [upper, bend]: Arm): string => {
    const wave = side === 1 && pose === POSES.wave ? Math.sin(t * 13) * 26 : 0
    const a1 = (upper * Math.PI) / 180
    const a2 = ((upper + bend + wave) * Math.PI) / 180
    const ex = SHOULDER.x + side * Math.sin(a1) * UPPER + boil(frame, `e${side}`)
    const ey = SHOULDER.y + Math.cos(a1) * UPPER + boil(frame, `ey${side}`)
    const hx = ex + side * Math.sin(a2) * FORE + boil(frame, `h${side}`)
    const hy = ey + Math.cos(a2) * FORE + boil(frame, `hy${side}`)
    return `M ${SHOULDER.x} ${SHOULDER.y} Q ${(SHOULDER.x + ex) / 2 + side * 3} ${(SHOULDER.y + ey) / 2 + 2} ${ex} ${ey} L ${hx} ${hy}`
  }
  const hand = (side: -1 | 1, [upper, bend]: Arm): [number, number] => {
    const wave = side === 1 && pose === POSES.wave ? Math.sin(t * 13) * 26 : 0
    const a1 = (upper * Math.PI) / 180
    const a2 = ((upper + bend + wave) * Math.PI) / 180
    return [
      SHOULDER.x + side * (Math.sin(a1) * UPPER + Math.sin(a2) * FORE),
      SHOULDER.y + Math.cos(a1) * UPPER + Math.cos(a2) * FORE,
    ]
  }

  const line = {
    stroke: STROKE,
    strokeWidth: 9,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    fill: 'none',
  }
  const [lhx, lhy] = hand(-1, pose.l)
  const [rhx, rhy] = hand(1, pose.r)
  const mouthW = 22 - open * 5
  const mouthUpper = 9 - open * 13
  const mouthLower = 11 + open * 30

  return (
    <svg
      width={w}
      height={h}
      viewBox={vb.join(' ')}
      preserveAspectRatio="xMidYMid meet"
      style={{ overflow: 'visible' }}
    >
      <g
        transform={`translate(0 ${-hop}) rotate(${sway} ${HIP.x} ${HIP.y})`}
        style={{ filter: 'drop-shadow(0 0 10px rgba(255,255,255,0.12))' }}
      >
        {/* Legs and spine. */}
        <path d={`M ${HIP.x} ${HIP.y} L ${150 + boil(frame, 'lf')} ${588 + hop * 0.3}`} {...line} />
        <path d={`M ${HIP.x} ${HIP.y} L ${250 + boil(frame, 'rf')} ${588 + hop * 0.3}`} {...line} />
        <path d={`M ${200 + boil(frame, 'n')} 228 Q ${203 + boil(frame, 's')} 330 ${HIP.x} ${HIP.y}`} {...line} />
        <path d={arm(-1, pose.l)} {...line} />
        <path d={arm(1, pose.r)} {...line} />
        <circle cx={lhx} cy={lhy} r={8} fill={STROKE} />
        <circle cx={rhx} cy={rhy} r={8} fill={STROKE} />
        {/* Head, tilted toward whatever he's pointing at. */}
        <g transform={`translate(0 ${bob}) rotate(${pose.tilt + Math.sin(t * 1.3) * 1.5} 200 200)`}>
          <path d={wobblyCircle(200, 150, 76, frame)} {...line} fill={COLORS.bg} />
          <path
            d={`M 150 96 Q 168 ${66 + boil(frame, 'h1', 2)} 196 82 Q 210 ${62 + boil(frame, 'h2', 2)} 236 84 Q 248 76 256 96`}
            {...line}
            strokeWidth={8}
          />
          {/* Glasses. */}
          <g stroke={COLORS.mustard} strokeWidth={6} fill="none" strokeLinecap="round">
            <circle cx={172} cy={150} r={23} />
            <circle cx={228} cy={150} r={23} />
            <path d="M 195 148 Q 200 143 205 148" />
            <path d="M 149 146 L 128 140" />
            <path d="M 251 146 L 272 140" />
          </g>
          {/* Eyes (blinking) and eyebrows (lifting on emphasis). */}
          <g fill={STROKE}>
            <ellipse cx={174} cy={151} rx={5.5} ry={5.5 * blink} />
            <ellipse cx={230} cy={151} rx={5.5} ry={5.5 * blink} />
          </g>
          <path d={`M 156 ${118 - brow} Q 172 ${110 - brow} 186 ${116 - brow}`} {...line} strokeWidth={6} />
          <path d={`M 214 ${116 - brow} Q 228 ${110 - brow} 244 ${118 - brow}`} {...line} strokeWidth={6} />
          {/* Mouth: a smile when quiet, opening with the voice. */}
          <path
            d={`M ${200 - mouthW} 193 Q 200 ${193 + mouthUpper} ${200 + mouthW} 193 Q 200 ${193 + mouthLower} ${200 - mouthW} 193 Z`}
            stroke={STROKE}
            strokeWidth={6}
            strokeLinejoin="round"
            fill={open > 0.05 ? '#2a1418' : COLORS.bg}
          />
        </g>
      </g>
    </svg>
  )
}

function currentPose(timeline: ReturnType<typeof useTimeline>['timeline'], sectionId: string, t: number): Pose {
  if (t >= timeline.finalHit) return POSES.wave
  const list = GESTURES[sectionId] ?? []
  const idx = phraseIndexAt(timeline, sectionId, t, 0.2)
  const caps = timeline.captions.filter((c) => c.sectionId === sectionId)
  const name = idx >= 0 ? (list[idx] ?? 'idle') : 'idle'
  const prev = idx > 0 ? (list[idx - 1] ?? 'idle') : 'idle'
  // Blend into the new pose over a quarter second.
  const since = idx >= 0 ? t - (caps[idx].start - 0.2) : 1
  const p = Math.min(1, since / 0.25)
  const e = p * p * (3 - 2 * p)
  return mix(POSES[prev], POSES[name], e)
}

function mix(a: Pose, b: Pose, p: number): Pose {
  const m = (x: number, y: number): number => x + (y - x) * p
  // Keep identity when settled, so the wave check (pose === POSES.wave) still works.
  if (p >= 1) return b
  return {
    l: [m(a.l[0], b.l[0]), m(a.l[1], b.l[1])],
    r: [m(a.r[0], b.r[0]), m(a.r[1], b.r[1])],
    tilt: m(a.tilt, b.tilt),
    hop: m(a.hop, b.hop),
  }
}

/** 1 = open, dips to ~0 for a few frames every few seconds (deterministic, slightly irregular). */
function blinking(t: number): number {
  const period = 3.1
  const k = Math.floor(t / period)
  const at = k * period + random(`blink${k}`) * 1.8
  const d = t - at
  return d >= 0 && d < 0.14 ? (Math.abs(d - 0.07) / 0.07) * 0.9 + 0.1 : 1
}

function wobblyCircle(cx: number, cy: number, r: number, frame: number): string {
  const n = 14
  const pts = Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2
    const rr = r + boil(frame, `head${i}`, 1.8)
    return [cx + Math.cos(a) * rr, cy + Math.sin(a) * rr * 1.04]
  })
  // Smooth closed curve through midpoints.
  const mid = (i: number): number[] => {
    const a = pts[i % n]
    const b = pts[(i + 1) % n]
    return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]
  }
  let d = `M ${mid(0)[0]} ${mid(0)[1]}`
  for (let i = 1; i <= n; i++) d += ` Q ${pts[i % n][0]} ${pts[i % n][1]} ${mid(i)[0]} ${mid(i)[1]}`
  return d + ' Z'
}
