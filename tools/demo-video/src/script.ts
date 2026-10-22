// The video's script: the single source for the voice generator (`scripts/voice.ts`) and the composition.
// Pure data, no imports, so Node can load it directly with type stripping.
//
// Each section is one voice file. `phrases` are the burned-in captions: `say` is what the TTS reads (spelled for
// pronunciation), `show` is what the caption shows (spelled for the eye, per the style guide). Caption timing comes
// from word timestamps of the generated audio, so phrases only need to follow the spoken order.

export type ShotKind = 'still' | 'video'

export type Shot = {
  /** Footage beat from `brand/clip-masters/video/video-beats.json`; the still is the fallback until it exists. */
  beat: string
  still: string
  /** Fraction of the section's footage time this shot takes (shots split the section in order). */
  weight: number
  /** Punch-in target on the still, in still-relative coordinates (0–1), and how far to zoom. */
  focus?: { x: number; y: number; zoom: number }
  /** Footage-relative: a named moment from the manifest to land the punch-in on. */
  moment?: string
  /** This shot cuts in exactly on the music's big drop (other shot cuts snap to the nearest bar line). */
  startsOnDrop?: boolean
}

export type Section = {
  id: string
  kind: 'intro' | 'beat' | 'close'
  /** The kinetic title card that opens a demo beat. */
  title?: string
  phrases: { say: string; show: string }[]
  /** A phrase index whose first word should land exactly on a downbeat (the music's big drop rides on it). */
  dropOnPhrase?: number
  /** The section runs at least this many bars (1 bar ≈ 1.7 s), so the footage gets time beyond the voice. */
  minBars?: number
  shots: Shot[]
}

export const SCRIPT: Section[] = [
  {
    id: 'intro',
    kind: 'intro',
    phrases: [
      { say: "Hi, I'm David.", show: "Hi, I'm David." },
      { say: 'I used Total Commander for twenty-five years.', show: 'I used Total Commander for 25 years.' },
      { say: 'Then I moved to a Mac,', show: 'Then I moved to a Mac,' },
      { say: 'and there was nothing like it.', show: 'and there was nothing like it.' },
      { say: 'So I built Commander.', show: 'So I built Cmdr.' },
    ],
    shots: [],
  },
  {
    id: 'speed',
    kind: 'beat',
    minBars: 6,
    title: 'Fast.',
    phrases: [
      { say: "It's built in Rust,", show: "It's built in Rust," },
      { say: "and it's fast.", show: "and it's fast." },
      { say: 'Fifty thousand files?', show: '50,000 files?' },
      { say: 'Instant.', show: 'Instant.' },
      { say: 'Search your whole drive as you type.', show: 'Search your whole drive as you type.' },
    ],
    shots: [
      { beat: 'speed', still: 'app-main-dark.webp', weight: 0.55, focus: { x: 0.28, y: 0.55, zoom: 1.3 } },
      { beat: 'speed', still: 'search-dark.webp', weight: 0.45, focus: { x: 0.5, y: 0.4, zoom: 1.2 } },
    ],
  },
  {
    id: 'trust',
    kind: 'beat',
    title: 'Undo anything.',
    phrases: [
      { say: 'Copying a big folder?', show: 'Copying a big folder?' },
      { say: 'Pause it.', show: 'Pause it.' },
      { say: 'Resume it.', show: 'Resume it.' },
      { say: 'Changed your mind?', show: 'Changed your mind?' },
      { say: 'Roll it back,', show: 'Roll it back,' },
      { say: 'and watch every file go back.', show: 'and watch every file go back.' },
      { say: 'Even days later.', show: 'Even days later.' },
    ],
    dropOnPhrase: 4,
    shots: [
      { beat: 'trust', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.5, y: 0.5, zoom: 1.1 } },
      {
        beat: 'trust',
        still: 'app-main-dark.webp',
        weight: 0.5,
        focus: { x: 0.68, y: 0.6, zoom: 1.4 },
        moment: 'rollback',
        startsOnDrop: true,
      },
    ],
  },
  {
    id: 'rename',
    kind: 'beat',
    minBars: 6,
    title: 'Rename in bulk.',
    phrases: [
      { say: 'Rename hundreds of files at once,', show: 'Rename hundreds of files at once,' },
      { say: 'with patterns and counters,', show: 'with patterns and counters,' },
      { say: 'and see every new name before you apply.', show: 'and see every new name before you apply.' },
    ],
    shots: [{ beat: 'rename', still: 'app-main-dark.webp', weight: 1, focus: { x: 0.3, y: 0.4, zoom: 1.32 } }],
  },
  {
    id: 'reach',
    kind: 'beat',
    minBars: 7,
    title: 'Reach everything.',
    phrases: [
      { say: 'It reaches everything:', show: 'It reaches everything:' },
      { say: 'your NAS, way faster than Finder.', show: 'your NAS, way faster than Finder.' },
      { say: 'Your Android phone.', show: 'Your Android phone.' },
      { say: 'Servers, S three, zip files,', show: 'Servers, S3, zip files,' },
      { say: 'even your Git history,', show: 'even your Git history,' },
      { say: 'all as plain folders.', show: 'all as plain folders.' },
    ],
    shots: [
      { beat: 'reach', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.28, y: 0.2, zoom: 1.45 } },
      { beat: 'reach', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.7, y: 0.45, zoom: 1.25 } },
    ],
  },
  {
    id: 'ai',
    kind: 'beat',
    minBars: 7,
    title: 'AI, if you want it.',
    phrases: [
      { say: "There's AI too,", show: "There's AI too," },
      { say: 'but only if you want it.', show: 'but only if you want it.' },
      { say: "It's off by default,", show: "It's off by default," },
      { say: 'and it can run fully on your Mac.', show: 'and it can run fully on your Mac.' },
      { say: 'Find my tax report from last year?', show: '"Find my tax report from last year"' },
      { say: 'Just works.', show: 'just works.' },
    ],
    shots: [{ beat: 'ai', still: 'chat-dark.webp', weight: 1, focus: { x: 0.6, y: 0.5, zoom: 1.3 } }],
  },
  {
    id: 'care',
    kind: 'beat',
    minBars: 6,
    title: 'Made with care.',
    phrases: [
      { say: 'Fifteen languages,', show: '15 languages,' },
      { say: 'real dark and light modes,', show: 'real dark and light modes,' },
      { say: 'VoiceOver,', show: 'VoiceOver,' },
      {
        say: 'and every single thing works from the keyboard.',
        show: 'and every single thing works from the keyboard.',
      },
    ],
    shots: [
      { beat: 'care', still: 'settings-dark.webp', weight: 0.5, focus: { x: 0.5, y: 0.45, zoom: 1.15 } },
      { beat: 'care', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.5, y: 0.9, zoom: 1.32 } },
    ],
  },
  {
    id: 'close',
    kind: 'close',
    phrases: [
      { say: 'Commander.', show: 'Cmdr.' },
      { say: 'Free for personal use.', show: 'Free for personal use.' },
      { say: 'Get it at get commander dot com.', show: 'Get it at getcmdr.com' },
    ],
    shots: [{ beat: 'hero', still: 'app-main-dark.webp', weight: 1 }],
  },
]

/** How the TTS should perform. Kept beside the script because it's part of the performance. */
export const VOICE_INSTRUCTIONS = [
  'Voice: a warm, natural man in his early forties, a developer talking about something he built and loves.',
  'Tone: friendly, a little playful, quietly confident. Never salesy, never announcer-like, no radio voice.',
  'Pacing: brisk and energetic but relaxed, with short natural pauses between sentences; punchy on one-word lines',
  'like "Instant." and "Pause it."',
  'Pronunciation: "Commander" is the product name, said as one ordinary word. "NAS" rhymes with "jazz".',
].join(' ')
