// Lays out a whole video from its spec, the voice takes, and the music's beat grid. Pure and deterministic: change
// the script, the voice, or the music anchors, and every cut, caption, and music edit follows.
//
// The rules:
// - The intro opens with the logo sting; the intro-to-first-beat cut lands on the music's `drop1`.
// - Beat sections start and end on bar lines, so every cut is on the beat. A title card lands on the cut and the
//   voice starts LEAD seconds later; `minBars` gives the footage room beyond the voice.
// - With a `lift`, one phrase (`dropOnPhrase`) gets the big drop: the music jumps forward by whole bars so the drop
//   lands in the pause just before that phrase, and the phrase starts two beats into the drop.
// - The close ends on the track's final hit (the logo slams on it), reached with a whole-bar jump (plus a measured
//   nudge for live music) into the track's last section.
import { FPS } from './config.ts'
import { framingProblems } from './lib/framing.ts'
import type { Section, Shot, VideoSpec } from './videos/types.ts'

export type VoicePhrase = { show: string; start: number; end: number }
/** `lead` is the silence trimmed off the take's start: a webcam take's video starts that much earlier. */
export type VoiceSection = { file: string; duration: number; lead: number; phrases: VoicePhrase[]; envelope: number[] }
export type VoiceManifest = { fps: number; source: string; sections: Record<string, VoiceSection> }

/** `moments`: the recording's labelled notes, plus `start`/`end` (the usable span) and `key` (its key frame). */
export type FootageBeat = {
  file: string
  duration: number
  width: number
  height: number
  moments: Record<string, number>
}
export type FootageManifest = { beats: Record<string, FootageBeat> }

/** A moment by exact key, else the first label containing it (case-insensitive). */
export function momentTime(f: FootageBeat, name: string): number {
  if (f.moments[name] !== undefined) return f.moments[name]
  const hit = Object.keys(f.moments).find((k) => k.toLowerCase().includes(name.toLowerCase()))
  if (hit === undefined)
    throw new Error(`Footage ${f.file} has no moment matching "${name}": ${Object.keys(f.moments).join(' | ')}`)
  return f.moments[hit]
}

export type VoiceClip = { sectionId: string; file: string; start: number; from: number; to: number }
export type MusicSegment = { start: number; end: number; trackOffset: number }
export type Caption = { show: string; start: number; end: number; sectionId: string }
/**
 * `footage.from` is the clip time at the shot's start. It can be before `usableStart` (the shot holds the first usable
 * frame until the clip catches up) or run past `duration` (the shot holds the last frame).
 */
export type ShotPlan = Shot & {
  start: number
  end: number
  /** When the synced moment plays (video time); the punch-in lands on it. */
  syncAt?: number
  footage?: { file: string; from: number; duration: number; aspect: number; usableStart: number }
}
export type SectionPlan = {
  id: string
  kind: Section['kind']
  title?: string
  index: number
  start: number
  end: number
  /** When the title card hands over to the footage. */
  footageStart: number
  shots: ShotPlan[]
}

export type Timeline = {
  duration: number
  sections: SectionPlan[]
  voice: VoiceClip[]
  captions: Caption[]
  music: MusicSegment[]
  /** Video times of the first drop, the lift (null without one), and the final hit. */
  drop1: number
  bigDrop: number | null
  finalHit: number
  /** Every downbeat of the computed grid, in video time. */
  downbeats: number[]
  /** What the pulse effects ride: the downbeats, or the track's real drum hits mapped into video time. */
  pulses: number[]
  /** Intervals where the music runs at full energy (pulses are on there). */
  highEnergy: [number, number][]
}

/** Title card alone on screen before the voice starts, and the footage's breathing room after the voice. */
const LEAD = 0.9
const BEAT_TAIL = 0.9
/** Gap between the last word and the cut (intro) or the final hit (close). */
const TAIL = 0.35
const STING = 1.9
const TITLE_CARD = 1.05

/**
 * `footage`: every video's footage manifest by video id (shots can borrow another video's recordings).
 * `onsets`: the track's strong drum hits in track seconds (`public/<id>/music-onsets.json`), for `pulse: 'onsets'`.
 */
export function buildTimeline(
  spec: VideoSpec,
  voice: VoiceManifest,
  footage: Record<string, FootageManifest>,
  onsets: number[] = [],
): Timeline {
  const { script, music } = spec
  const beat = 60 / music.bpm
  const bar = beat * 4
  const v = (id: string): VoiceSection => {
    const s = voice.sections[id]
    if (!s) throw new Error(`No voice for section "${id}". Run: pnpm voice --video=${spec.id}`)
    return s
  }

  const sections: SectionPlan[] = []
  const voiceClips: VoiceClip[] = []
  const captions: Caption[] = []
  const place = (id: string, start: number, from = 0, to = v(id).duration): void => {
    voiceClips.push({ sectionId: id, file: v(id).file, start, from, to })
    for (const p of v(id).phrases)
      if (p.start >= from - 0.01 && p.end <= to + 0.01)
        captions.push({ show: p.show, start: start + p.start - from, end: start + p.end - from, sectionId: id })
  }

  // Intro: sting, then the voice; the first drop lands right after it.
  const intro = script[0]
  place(intro.id, STING)
  const drop1 = STING + v(intro.id).duration + TAIL
  const downbeat = (k: number): number => drop1 + k * bar
  const snapUp = (t: number, step: number): number => drop1 + Math.ceil((t - drop1 - 1e-6) / step) * step
  sections.push({ id: intro.id, kind: 'intro', index: 0, start: 0, end: drop1, footageStart: 0, shots: [] })

  // Beat sections.
  let cursor = drop1
  let bigDrop: number | null = null
  let liftBar = 0
  const close = script[script.length - 1]
  for (const [i, section] of script.slice(1, -1).entries()) {
    const sv = v(section.id)
    const start = cursor
    let voiceEnd: number
    if (music.lift && section.dropOnPhrase !== undefined) {
      const d = section.dropOnPhrase
      const split = (sv.phrases[d - 1].end + sv.phrases[d].start) / 2
      // Part A ends just before the drop; part B's phrase starts two beats into it.
      liftBar = Math.ceil((start + LEAD + split + 0.3 - drop1) / bar)
      bigDrop = downbeat(liftBar)
      place(section.id, bigDrop - 0.3 - split, 0, split)
      const bStart = bigDrop + 2 * beat - (sv.phrases[d].start - split)
      place(section.id, bStart, split, sv.duration)
      voiceEnd = bStart + (sv.duration - split)
    } else {
      place(section.id, start + LEAD)
      voiceEnd = start + LEAD + sv.duration
    }
    const end = Math.max(snapUp(voiceEnd + BEAT_TAIL, bar * music.cutEvery), start + (section.minBars ?? 0) * bar)
    sections.push({
      id: section.id,
      kind: 'beat',
      title: section.title,
      index: i + 1,
      start,
      end,
      footageStart: start + TITLE_CARD,
      shots: [],
    })
    cursor = end
  }

  // Music, part one: from the intro on, with drop1 on the cut. With a lift, jump forward by whole bars into the last
  // `keepBars` of its build, so the lift lands on `bigDrop`.
  const segments: MusicSegment[] = []
  const o1 = music.drop1 - drop1
  let oPrev = o1
  let segStart = 0
  if (music.lift && bigDrop !== null) {
    const jumpAt = downbeat(liftBar - music.lift.keepBars)
    segments.push({ start: 0, end: jumpAt, trackOffset: o1 })
    oPrev = music.drop1 + (music.lift.bar - music.lift.keepBars) * bar - jumpAt
    segStart = jumpAt
  }

  // The close: voice ends TAIL before the final hit; the hit comes from a whole-bar jump into the final section.
  const closeVoice = v(close.id)
  const closeStart = cursor
  const earliestHit = closeStart + 0.6 + closeVoice.duration + TAIL
  const n = Math.floor((music.finalHit - oPrev - music.finalJumpNudge - earliestHit) / bar)
  const o3 = oPrev + n * bar + music.finalJumpNudge
  const finalHit = music.finalHit - o3
  place(close.id, finalHit - TAIL - closeVoice.duration)
  const duration = finalHit + music.tail
  // Jump into the final section on the first downbeat of the last beat section or later, as the track allows.
  const lastBeat = sections[sections.length - 1]
  const jump2 = snapUp(Math.max(lastBeat.start, music.finalSection - o3), bar)
  segments.push({ start: segStart, end: jump2, trackOffset: oPrev })
  segments.push({ start: jump2, end: duration, trackOffset: o3 })
  sections.push({
    id: close.id,
    kind: 'close',
    index: script.length - 1,
    start: closeStart,
    end: duration,
    footageStart: closeStart,
    shots: [],
  })

  // Shots. Cut points come from cues (just before a phrase, on the beat), the lift, or the shots' weights (on a bar
  // line). Real footage replaces the still when the manifest has it, lined up with the voice by `sync`.
  const snapBeat = (t: number): number => drop1 + Math.round((t - drop1) / beat) * beat
  for (const [si, plan] of sections.entries()) {
    const section = script[si]
    const phraseStart = (k: number): number => {
      const c = captions.filter((x) => x.sectionId === section.id)[k]
      if (!c) throw new Error(`${section.id}: no phrase ${k}`)
      return c.start
    }
    const total = plan.end - plan.footageStart
    const weights = section.shots.reduce((s, x) => s + x.weight, 0)
    const bounds = [plan.footageStart]
    let acc = plan.footageStart
    for (const [k, shot] of section.shots.entries()) {
      if (k === 0) {
        acc += (shot.weight / weights) * total
        continue
      }
      const byWeight = drop1 + Math.round((acc - drop1) / bar) * bar
      const cut =
        shot.startsOnDrop && bigDrop !== null
          ? bigDrop
          : shot.cue !== undefined
            ? snapBeat(phraseStart(shot.cue) - 0.3)
            : byWeight
      const prev = bounds[bounds.length - 1]
      bounds.push(cut > prev + 0.4 && cut < plan.end - 0.4 ? cut : Math.min(plan.end - 0.4, Math.max(prev + 0.4, acc)))
      acc += (shot.weight / weights) * total
    }
    bounds.push(plan.end)
    const playedTo: Record<string, number> = {}
    plan.shots = section.shots.map((shot, k) => {
      const start = bounds[k]
      const end = bounds[k + 1]
      const own = footage[spec.id]?.beats ?? {}
      const primary = footage[shot.source ?? spec.id]?.beats[shot.beat]
      const fallback = primary ? undefined : shot.fallbackBeat ? own[shot.fallbackBeat] : undefined
      const f = primary ?? fallback
      const played = primary ? `${shot.source ?? spec.id}:${shot.beat}` : `${spec.id}:${shot.fallbackBeat}`
      if (!f || shot.stillOnly) return { ...shot, start, end }
      let from: number
      let syncAt: number | undefined
      if (shot.sync && primary) {
        const m = momentTime(f, shot.sync.moment)
        const at = shot.sync.phrase !== undefined ? phraseStart(shot.sync.phrase) : start + 0.3
        from = m - (at - start)
        syncAt = at
      } else from = playedTo[played] ?? f.moments.start ?? 0
      playedTo[played] = from + (end - start)
      return {
        ...shot,
        start,
        end,
        syncAt,
        footage: {
          file: f.file,
          from,
          duration: f.duration,
          aspect: f.width / f.height,
          usableStart: f.moments.start ?? 0,
        },
      }
    })
  }

  const downbeats: number[] = []
  for (let k = -Math.ceil(drop1 / bar); downbeat(k) < duration; k++) if (downbeat(k) >= 0) downbeats.push(downbeat(k))
  // Real drum hits, mapped through the edit's segments into video time.
  const hits = segments.flatMap((s) => onsets.map((t) => t - s.trackOffset).filter((t) => t >= s.start && t < s.end))

  const highEnergy: [number, number][] =
    music.lift && bigDrop !== null
      ? // Drop one runs until the breakdown, which is quiet, then the big drop runs to the end.
        [
          [drop1, Math.min(segments[0].end, downbeat(music.lift.breakdownBar))],
          [bigDrop, finalHit + 0.1],
        ]
      : [[drop1, finalHit + 0.1]]

  const timeline: Timeline = {
    duration,
    sections,
    voice: voiceClips,
    captions,
    music: segments,
    drop1,
    bigDrop,
    finalHit,
    downbeats,
    pulses: spec.theme.pulse === 'onsets' && hits.length > 0 ? hits : downbeats,
    highEnergy,
  }
  // No shot may put the app window off-canvas (`lib/framing.ts`): fail here, before a render ships a broken frame.
  const problems = framingProblems(timeline, spec)
  if (problems.length > 0) throw new Error(`${spec.id}: framing problems:\n  ${problems.join('\n  ')}`)
  return timeline
}

/** Seconds to frames, rounded the same way everywhere. */
export const f = (seconds: number): number => Math.round(seconds * FPS)
