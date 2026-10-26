// Where the app window sits on the canvas, how a punch-in frames it, and the check that no frame ever puts the
// window off-canvas. Pure math with no Remotion imports, so `buildTimeline` (and the Node scripts) can verify every
// frame of every shot before anything renders.
//
// The window is a card that never moves on the beat: only its gold glow pulses. A punch-in is slow and lands on the
// shot's synced moment. It first grows the card toward the focus point, but only as far as the canvas's safe area
// allows; any zoom beyond that happens INSIDE the card (clipped by it), clamped so the footage always covers the card.
// So whatever a shot's `focus` says, the window stays whole and on-canvas.
import { FPS, HEIGHT, WIDTH } from '../config.ts'
import type { SectionPlan, ShotPlan, Timeline } from '../timeline.ts'
import type { VideoSpec } from '../videos/types.ts'

export type Rect = { left: number; top: number; width: number; height: number }
type Point = { x: number; y: number }

/** The settled window card: centered a bit high, leaving the bottom band for captions; at most this wide and tall. */
const BOX = { cx: 960, cy: 488, maxW: 1440, maxH: 780 }
/** The stills carry the window's shadow margins, so their card is the full image. */
export const STILL_ASPECT = 2508 / 1634
/** The window keeps at least this far from every canvas edge, in every frame. */
export const SAFE_MARGIN = 24

export function windowRect(aspect: number): Rect {
  const width = Math.min(BOX.maxW, BOX.maxH * aspect)
  const height = width / aspect
  return { left: BOX.cx - width / 2, top: BOX.cy - height / 2, width, height }
}

const smooth = (x: number): number => {
  const c = Math.min(1, Math.max(0, x))
  return c * c * (3 - 2 * c)
}
const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v))

/** The window's entrance after the title card: 0 (just cut in) → 1 (settled), easing out. */
export function revealAt(section: SectionPlan, t: number): number {
  const x = clamp((t - section.footageStart + 0.3) / 0.55, 0, 1)
  return 1 - Math.pow(1 - x, 4)
}

/** A punch-in is a slow, deliberate move: it eases in over this long and settles just after the synced moment. */
const PUNCH_SECONDS = 1.4
const PUNCH_SETTLE = 0.5

/**
 * How far into its punch-in a shot is at video time `t`: 0 shows the whole window, 1 the shot's full `focus` zoom.
 * The punch lands on the synced moment; a shot without one holds the whole window. `closeFraming` holds the full
 * punch-in for the whole shot, so whatever it crops (like the SMB test server's address) never shows.
 */
export function punchProgress(shot: ShotPlan, t: number): number {
  if (shot.closeFraming) return 1
  if (shot.syncAt === undefined || !shot.focus) return 0
  const end = shot.syncAt + PUNCH_SETTLE
  return smooth((t - (end - PUNCH_SECONDS)) / PUNCH_SECONDS)
}

/**
 * A shot's framing at video time `t`:
 * - `card`: the window card on the canvas, grown by the punch-in toward its focus point within the safe area.
 *   `closeFraming` keeps the card at its settled size, so the whole punch-in crops (that's what keeps something out of
 *   frame).
 * - `zoom`, `x`, `y`: the footage inside the card (CSS `translate(x, y) scale(zoom)`, origin at the card's center),
 *   the rest of the punch-in, clamped so the footage always covers the card.
 * - `enter`: the entrance (`scale` about the canvas center, then `x`), capped so even the entering card stays inside.
 */
export type Framing = {
  card: Rect
  zoom: number
  x: number
  y: number
  enter: { scale: number; x: number }
}

export function shotFraming(
  shot: ShotPlan,
  section: SectionPlan,
  transition: VideoSpec['theme']['transition'],
  t: number,
): Framing {
  const base = windowRect(shot.footage?.aspect ?? STILL_ASPECT)
  const focus = shot.focus ?? { x: 0.5, y: 0.5, zoom: 1 }
  const p = punchProgress(shot, t)
  const total = 1 + (Math.max(1, focus.zoom) - 1) * p
  const m = SAFE_MARGIN
  const fit = Math.min((WIDTH - 2 * m) / base.width, (HEIGHT - 2 * m) / base.height)
  const s = Math.min(total, shot.closeFraming ? 1 : fit)
  const w = base.width * s
  const h = base.height * s
  const c0 = { x: base.left + base.width / 2, y: base.top + base.height / 2 }
  // A full punch-in would shift the focus point by `d` to center it. The card takes the share of that its growth
  // covers (none when it can't grow), within the safe area; the footage inside the card takes the rest.
  const d = { x: (0.5 - focus.x) * base.width * total * p, y: (0.5 - focus.y) * base.height * total * p }
  const share = total > 1 ? (s - 1) / (total - 1) : 0
  const cx = clamp(c0.x + d.x * share, m + w / 2, WIDTH - m - w / 2)
  const cy = clamp(c0.y + d.y * share, m + h / 2, HEIGHT - m - h / 2)
  const inner = total / s
  const ix = c0.x + d.x - cx
  const iy = c0.y + d.y - cy
  const card = { left: cx - w / 2, top: cy - h / 2, width: w, height: h }
  return {
    card,
    zoom: inner,
    x: clamp(ix, (-(inner - 1) * w) / 2, ((inner - 1) * w) / 2),
    y: clamp(iy, (-(inner - 1) * h) / 2, ((inner - 1) * h) / 2),
    enter: entrance(transition, revealAt(section, t), card),
  }
}

/**
 * The card's entrance, at `reveal` 0 (just cut in) → 1 (settled): `zoom` settles it from a little larger (about the
 * canvas center), `slide` from the right. Both are capped so the entering card stays inside the safe area.
 */
function entrance(
  transition: VideoSpec['theme']['transition'],
  reveal: number,
  card: Rect,
): { scale: number; x: number } {
  const m = SAFE_MARGIN
  if (transition === 'slide') return { scale: 1, x: Math.min((1 - reveal) * 220, WIDTH - m - card.left - card.width) }
  const room = Math.min(
    (WIDTH / 2 - m) / (WIDTH / 2 - card.left),
    (WIDTH / 2 - m) / (card.left + card.width - WIDTH / 2),
    (HEIGHT / 2 - m) / (HEIGHT / 2 - card.top),
    (HEIGHT / 2 - m) / (card.top + card.height - HEIGHT / 2),
  )
  return { scale: Math.max(1, Math.min(1.12 - 0.12 * reveal, room)), x: 0 }
}

/** The close's floating app: a static card turning slowly in perspective, hinged on its left edge. */
export const CLOSE = { left: 820, top: 190, width: 940, perspective: 1800 }

export function closeTurn(local: number): { rotateY: number; rotateX: number } {
  return { rotateY: -16 + Math.min(local, 10) * 0.5, rotateX: 4 - Math.min(local, 10) * 0.2 }
}

/**
 * The close card's four corners on the canvas, projected the way CSS does it: `rotateY(ry) rotateX(rx)` about the
 * left edge's middle, under `perspective` from the canvas center.
 */
export function closeCorners(aspect: number, local: number): Point[] {
  const { rotateY, rotateX } = closeTurn(local)
  const height = CLOSE.width / aspect
  const ox = CLOSE.left
  const oy = CLOSE.top + height / 2
  const ry = (rotateY * Math.PI) / 180
  const rx = (rotateX * Math.PI) / 180
  return [
    [0, 0],
    [CLOSE.width, 0],
    [0, height],
    [CLOSE.width, height],
  ].map(([cx, cy]) => {
    const x0 = CLOSE.left + cx - ox
    const y0 = CLOSE.top + cy - oy
    // rotateX first (it's the inner transform), then rotateY; +z points at the viewer.
    const y1 = y0 * Math.cos(rx)
    const z1 = y0 * Math.sin(rx)
    const x2 = x0 * Math.cos(ry) + z1 * Math.sin(ry)
    const z2 = -x0 * Math.sin(ry) + z1 * Math.cos(ry)
    const k = CLOSE.perspective / (CLOSE.perspective - z2)
    return { x: WIDTH / 2 + (ox + x2 - WIDTH / 2) * k, y: HEIGHT / 2 + (oy + y1 - HEIGHT / 2) * k }
  })
}

const offCanvas = (pts: Point[]): string | null => {
  const left = Math.min(...pts.map((p) => p.x))
  const right = Math.max(...pts.map((p) => p.x))
  const top = Math.min(...pts.map((p) => p.y))
  const bottom = Math.max(...pts.map((p) => p.y))
  const m = SAFE_MARGIN
  return left < m || top < m || right > WIDTH - m || bottom > HEIGHT - m
    ? `window spans x ${left.toFixed(0)}–${right.toFixed(0)}, y ${top.toFixed(0)}–${bottom.toFixed(0)}`
    : null
}

/**
 * Every framing problem in the edit, checked frame by frame: the window card (entrance included) and the close's
 * turning card must stay `SAFE_MARGIN` inside the canvas, and a punch-in must keep its footage covering the card with
 * the focus point inside it. `buildTimeline` throws on any, so a bad shot fails before it renders.
 */
export function framingProblems(timeline: Timeline, spec: VideoSpec): string[] {
  const problems: string[] = []
  const at = (t: number): string => `${t.toFixed(2)} s`
  for (const section of timeline.sections)
    for (const shot of section.shots) {
      const aspect = shot.footage?.aspect ?? STILL_ASPECT
      if (section.kind === 'close') {
        for (let t = shot.start; t < shot.end; t += 1 / FPS) {
          const off = offCanvas(closeCorners(aspect, t - section.start))
          if (off) {
            problems.push(`${section.id} close card at ${at(t)}: ${off}`)
            break
          }
        }
        continue
      }
      const focus = shot.focus
      if (focus && (focus.x < 0 || focus.x > 1 || focus.y < 0 || focus.y > 1 || focus.zoom < 1 || focus.zoom > 2))
        problems.push(
          `${section.id} shot at ${at(shot.start)}: focus ${JSON.stringify(focus)} is outside 0–1 or zoom 1–2`,
        )
      for (let t = shot.start; t < shot.end; t += 1 / FPS) {
        const { card, zoom, x, y, enter } = shotFraming(shot, section, spec.theme.transition, t)
        const off = offCanvas(
          [
            { x: card.left, y: card.top },
            { x: card.left + card.width, y: card.top + card.height },
          ].map((p) => ({
            x: WIDTH / 2 + (p.x - WIDTH / 2) * enter.scale + enter.x,
            y: HEIGHT / 2 + (p.y - HEIGHT / 2) * enter.scale,
          })),
        )
        const gapX = ((zoom - 1) * card.width) / 2 - Math.abs(x)
        const gapY = ((zoom - 1) * card.height) / 2 - Math.abs(y)
        // Where the focus point lands inside the card, 0–1.
        const fx = 0.5 + ((focus?.x ?? 0.5) - 0.5) * zoom + x / card.width
        const fy = 0.5 + ((focus?.y ?? 0.5) - 0.5) * zoom + y / card.height
        const problem = off
          ? off
          : gapX < -0.01 || gapY < -0.01
            ? 'the punch-in leaves the footage short of the card'
            : fx < 0 || fx > 1 || fy < 0 || fy > 1
              ? 'the punch-in loses its focus point'
              : null
        if (problem) {
          problems.push(`${section.id} shot at ${at(t)}: ${problem}`)
          break
        }
      }
    }
  return problems
}
