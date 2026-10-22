// Small motion helpers shared by the scenes. Everything is a pure function of time, so any frame renders alone.
import { Easing, interpolate, random } from 'remotion'

export const clamp01 = (x: number): number => Math.min(1, Math.max(0, x))

/** 0 → 1 between `from` and `to` seconds, eased. */
export const ramp = (t: number, from: number, to: number, easing = Easing.bezier(0.22, 1, 0.36, 1)): number =>
  interpolate(t, [from, to], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing })

/** A hit that jumps to 1 at `at` and decays exponentially (half-life in seconds). Zero before `at`. */
export const hit = (t: number, at: number, halfLife = 0.12): number => (t < at ? 0 : Math.pow(0.5, (t - at) / halfLife))

/** The strongest decaying hit among a list of times. */
export const hits = (t: number, times: number[], halfLife = 0.12): number =>
  times.reduce((m, at) => Math.max(m, hit(t, at, halfLife)), 0)

/** Deterministic hand-drawn "boil": a small offset that changes every few frames, like redrawn animation cels. */
export const boil = (frame: number, id: string, amount = 1.6, every = 3): number =>
  (random(`${id}-${Math.floor(frame / every)}`) - 0.5) * 2 * amount

export const inRanges = (t: number, ranges: [number, number][]): boolean => ranges.some(([a, b]) => t >= a && t < b)

export const COLORS = {
  bg: '#0a0a0b',
  surface: '#111113',
  elevated: '#18181b',
  border: '#27272a',
  text: '#fafafa',
  textDim: '#a1a1aa',
  mustard: '#ffc206',
  mustardHover: '#ffd23f',
  // The logo's back-panel haze.
  blue: '#3b82f6',
  violet: '#8b5cf6',
  pink: '#ec4899',
  cyan: '#22d3ee',
}

export const FONT = "'Inter', system-ui, -apple-system, sans-serif"
export const MONO = "ui-monospace, 'SF Mono', SFMono-Regular, Menlo, monospace"
