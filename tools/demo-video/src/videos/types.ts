// What makes one video: its script, voice direction, music edit, look, and gestures. Everything else (timeline,
// presenter, captions, effects, soundtrack) is shared machinery. Pure types, so Node scripts can import them.

export type Shot = {
  /** Footage beat from this video's `video-beats.json`; the still is the fallback until it exists. */
  beat: string
  still: string
  /** Fraction of the section's footage time this shot takes (shots split the section in order). */
  weight: number
  /** Punch-in target, in footage-relative coordinates (0–1), and how far to zoom. */
  focus?: { x: number; y: number; zoom: number }
  /** The shot cuts in just before this phrase (index in the section) starts, snapped to the nearest beat. */
  cue?: number
  /** This shot cuts in exactly on the music's lift. */
  startsOnDrop?: boolean
  /**
   * Lines the footage up with the voice: the clip moment whose manifest label contains `moment` (case-insensitive;
   * also `key` for the key frame, `start` for the usable start) plays exactly when phrase `phrase` starts, or 0.3 s
   * into the shot without a phrase. Without `sync`, the clip continues from where the beat's previous shot left off.
   */
  sync?: { moment: string; phrase?: number }
  /** Keep the still even when footage exists, with the reason (for example, a recording that shows the wrong thing). */
  stillOnly?: string
  /** Borrow `beat` from another video's recordings (its spec `id`). */
  source?: string
  /** When `beat`'s footage doesn't exist (yet), play this beat of this video's own recordings instead (no `sync`). */
  fallbackBeat?: string
}

export type Section = {
  id: string
  kind: 'intro' | 'beat' | 'close'
  /** The kinetic title card that opens a demo beat. */
  title?: string
  /**
   * The burned-in captions, in spoken order: `say` is what the TTS reads (spelled for pronunciation), `show` is what
   * the caption shows (spelled for the eye, per the style guide).
   */
  phrases: { say: string; show: string }[]
  /** A phrase whose first word lands two beats after the music's lift (the big drop rides on the pause before it). */
  dropOnPhrase?: number
  /** The section runs at least this many bars, so the footage gets time beyond the voice. */
  minBars?: number
  shots: Shot[]
}

export type PoseName = 'idle' | 'wave' | 'point' | 'open' | 'shrug' | 'cheer' | 'thumb'

/** The music edit. Times are in the TRACK's own seconds, measured with `scripts/beat-grid.ts` and `analyze-music.ts`. */
export type MusicSpec = {
  /** The downloaded track in `assets/music/` (see CREDITS.md); `pnpm prepare-assets` copies it into `public/<id>/`. */
  source: string
  /** The short on-screen credit for the end card (CC BY needs visible attribution). */
  credit: string
  bpm: number
  /** Section cuts snap to this many bars: 1 for fast tracks, 0.5 (two beats) when bars are long. */
  cutEvery: 1 | 0.5
  /** The downbeat that lands on the intro-to-first-beat cut (a drop, or the band coming in). */
  drop1: number
  /**
   * Optional big lift, `bar` bars after `drop1` in the track. The edit jumps forward by whole bars so it lands in the
   * pause before the `dropOnPhrase` line, keeping the last `keepBars` bars of the build before it.
   */
  lift?: { bar: number; keepBars: number; breakdownBar: number }
  /** Start of the track's last section; the ending jumps into it so the final hit lands on the logo. */
  finalSection: number
  finalHit: number
  /** Seconds of ring-out kept after the final hit. */
  tail: number
  /** Crossfade at the edit's jumps, in seconds. A grid-tight electronic track can hard-cut; live drums need a blend. */
  joinFade: number
  /** Fine alignment added to the ending jump on top of whole bars (live tempo drifts); from `scripts/align-jump.ts`. */
  finalJumpNudge: number
  gainUnderVoice: number
  gainOpen: number
  /** The final hit and its ring-out. Keep the mix ~2 dB below full scale (`pnpm check-levels`). */
  gainFinal: number
}

/** The look. The first video is drop-and-glitch; the second is warmer and rides the drummer. */
export type Theme = {
  /** `slam`: letters fly up and the card zooms through. `groove`: words step in on the beat, a marker swipes. */
  titleStyle: 'slam' | 'groove'
  /** Card-to-footage move: a zoom-through, or a sideways push. */
  transition: 'zoom' | 'slide'
  /** White flashes on the drops. */
  flashes: boolean
  /** What the footage, haze, and bubble pulse to: computed downbeats, or the track's real drum hits. */
  pulse: 'downbeats' | 'onsets'
  pulseAmount: number
  /** Backdrop haze colors, and the light-leak color pairs that sweep through on cuts. */
  haze: [string, string, string, string]
  leaks: [string, string][]
}

export type VideoSpec = {
  /** Folder name under `public/` and `assets/voice-raw/`, and the CLI's `--video=` value. */
  id: string
  compositionId: string
  /** Folder under `brand/clip-masters/` holding this video's `video-beats.json` and ProRes masters. */
  footageDir: string
  script: Section[]
  /** How the TTS should perform (the `instructions` parameter). */
  voiceInstructions: string
  music: MusicSpec
  theme: Theme
  /** One pose per caption phrase, per section. */
  gestures: Record<string, PoseName[]>
  /** Which intro visual answers the presenter's lines. */
  intro: 'origin' | 'everywhere'
  /** A line under the URL on the end card. */
  endTagline: string
}
