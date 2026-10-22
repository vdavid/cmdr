// Global knobs. Pure data, no imports, so Node scripts can load it too.

export const FPS = 30
export const WIDTH = 1920
export const HEIGHT = 1080

/**
 * Who's on screen. `stick` is the drawn stick figure, lip-synced to the voice envelope. `webcam` plays
 * `public/webcam/<sectionId>.mp4` in the same slots (full frame at intro and close, the corner bubble in the beats).
 * Run `pnpm voice --source=webcam` after dropping the takes in, so the captions and timing follow the real takes.
 */
export const PRESENTER: { kind: 'stick' } | { kind: 'webcam' } = { kind: 'stick' }

/** The music edit. Times are in the TRACK's own seconds, measured with `scripts/beat-grid.ts` and `analyze-music.ts`. */
export const MUSIC = {
  /** The downloaded track in `assets/music/` (see CREDITS.md); `pnpm prepare-assets` copies it to `file`. */
  source: 'cdk-snaps.mp3',
  file: 'music/track.mp3',
  bpm: 139.94,
  /** First drop: lands on the intro-to-demo cut. */
  drop1: 13.5,
  /** Bars from the first drop to the breakdown, and to the big drop. */
  breakdownBar: 9,
  bigDropBar: 16,
  /** The big drop's own time, and the start of the final drop section we jump into for the ending. */
  bigDrop: 40.93,
  finalSection: 164.0,
  /** The last stab before the track's clean decay: the logo slams on it. */
  finalHit: 192.02,
  /** Seconds of decay we keep after the final hit. */
  tail: 2.6,
  /** Music level, as gain, under the voice and between lines. */
  gainUnderVoice: 0.13,
  gainOpen: 0.46,
  /** The final hit and its decay; with the drop punches (gainOpen × 1.15) this keeps the mix ~2 dB below full scale. */
  gainFinal: 0.66,
}

export const VOICE = {
  model: 'gpt-4o-mini-tts',
  voice: 'cedar',
  /** Integrated loudness the voice files are normalized to. */
  lufs: -16,
}
