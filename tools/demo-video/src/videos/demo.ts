// Video 1, "Cmdr in 90 seconds": David's origin story, then six beats of what only Cmdr does, cut to dubstep.
// Pure data (type-only imports), so Node scripts can load it with type stripping.
import type { Section, VideoSpec } from './types.ts'

const script: Section[] = [
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
      {
        beat: 'speed',
        still: 'app-main-dark.webp',
        weight: 0.55,
        focus: { x: 0.3, y: 0.5, zoom: 1.25 },
        sync: { moment: 'the 50,000-file listing is up', phrase: 2 },
      },
      {
        beat: 'speed',
        still: 'search-dark.webp',
        weight: 0.45,
        focus: { x: 0.5, y: 0.4, zoom: 1.25 },
        cue: 4,
        sync: { moment: 'search opens', phrase: 4 },
      },
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
      // The recording's Pause, Resume, and rollback each land on their line; the rollback cuts in on the big drop.
      {
        beat: 'trust',
        still: 'app-main-dark.webp',
        weight: 0.3,
        focus: { x: 0.5, y: 0.5, zoom: 1.1 },
        sync: { moment: 'F5: copy', phrase: 0 },
      },
      {
        beat: 'trust',
        still: 'app-main-dark.webp',
        weight: 0.15,
        focus: { x: 0.5, y: 0.55, zoom: 1.3 },
        cue: 1,
        sync: { moment: 'Pause', phrase: 1 },
      },
      {
        beat: 'trust',
        still: 'app-main-dark.webp',
        weight: 0.15,
        focus: { x: 0.5, y: 0.55, zoom: 1.2 },
        cue: 2,
        sync: { moment: 'Resume', phrase: 2 },
      },
      {
        beat: 'trust',
        still: 'app-main-dark.webp',
        weight: 0.4,
        focus: { x: 0.5, y: 0.55, zoom: 1.35 },
        startsOnDrop: true,
        sync: { moment: 'the bar walks back', phrase: 5 },
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
    shots: [
      {
        beat: 'rename',
        still: 'app-main-dark.webp',
        weight: 1,
        focus: { x: 0.5, y: 0.45, zoom: 1.25 },
        sync: { moment: 'typing the pattern', phrase: 1 },
      },
    ],
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
      // The NAS and the phone borrow short slices of the second video's recordings (until those exist, the calm hero
      // clip stands in); the zip and the Git history land on their words.
      {
        beat: 'smb',
        source: 'anywhere',
        fallbackBeat: 'hero',
        still: 'app-main-dark.webp',
        weight: 0.25,
        focus: { x: 0.5, y: 0.66, zoom: 1.4 },
        closeFraming: true,
        sync: { moment: 'listed: 5,000 files', phrase: 1 },
      },
      {
        beat: 'phone',
        source: 'anywhere',
        fallbackBeat: 'hero',
        still: 'app-main-dark.webp',
        weight: 0.15,
        focus: { x: 0.5, y: 0.5, zoom: 1.12 },
        cue: 2,
        sync: { moment: 'copying off the phone', phrase: 2 },
      },
      {
        beat: 'reach',
        still: 'app-main-dark.webp',
        weight: 0.3,
        focus: { x: 0.35, y: 0.45, zoom: 1.25 },
        cue: 3,
        sync: { moment: 'inside the zip', phrase: 3 },
      },
      {
        beat: 'reach',
        still: 'app-main-dark.webp',
        weight: 0.3,
        focus: { x: 0.35, y: 0.45, zoom: 1.25 },
        cue: 4,
        sync: { moment: 'Enter on .git', phrase: 4 },
      },
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
    shots: [
      // A real natural-language search with Cmdr's local model: the sentence types in during "off by default", Enter
      // (the model turning it into a name pattern and a date range) follows "run fully on your Mac", and last year's
      // tax files come back exactly on the quoted line.
      {
        beat: 'ai',
        still: 'chat-dark.webp',
        weight: 1,
        focus: { x: 0.5, y: 0.4, zoom: 1.25 },
        sync: { moment: "only last year's tax files come back", phrase: 4 },
      },
    ],
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
      // Languages preview live, the theme flips to light on "dark and light modes", then back to arrowing through
      // languages for "every single thing works from the keyboard".
      {
        beat: 'care',
        still: 'settings-dark.webp',
        weight: 0.3,
        focus: { x: 0.5, y: 0.4, zoom: 1.12 },
        sync: { moment: 'ArrowDown', phrase: 0 },
      },
      {
        beat: 'care',
        still: 'settings-dark.webp',
        weight: 0.3,
        focus: { x: 0.5, y: 0.45, zoom: 1.12 },
        cue: 1,
        sync: { moment: 'Theme mode', phrase: 1 },
      },
      {
        beat: 'care',
        still: 'settings-dark.webp',
        weight: 0.4,
        focus: { x: 0.5, y: 0.4, zoom: 1.12 },
        cue: 3,
        sync: { moment: 'ArrowDown', phrase: 3 },
      },
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

export const demo: VideoSpec = {
  id: 'demo',
  compositionId: 'CmdrDemo',
  footageDir: 'video',
  script,
  voiceInstructions: [
    'Voice: a warm, natural man in his early forties, a developer talking about something he built and loves.',
    'Tone: friendly, a little playful, quietly confident. Never salesy, never announcer-like, no radio voice.',
    'Pacing: brisk and energetic but relaxed, with short natural pauses between sentences; punchy on one-word lines',
    'like "Instant." and "Pause it."',
    'Pronunciation: "Commander" is the product name, said as one ordinary word. "NAS" rhymes with "jazz".',
  ].join(' '),
  music: {
    source: 'cdk-snaps.mp3',
    credit: 'Music: "Snaps" by cdk · CC BY 3.0',
    bpm: 139.94,
    cutEvery: 1,
    drop1: 13.5,
    lift: { bar: 16, keepBars: 2, breakdownBar: 9 },
    finalSection: 164.0,
    finalHit: 192.02,
    tail: 2.6,
    joinFade: 1 / 30,
    finalJumpNudge: -0.044,
    gainUnderVoice: 0.13,
    gainOpen: 0.46,
    gainFinal: 0.66,
  },
  theme: {
    titleStyle: 'slam',
    transition: 'zoom',
    flashes: true,
    pulse: 'downbeats',
    pulseAmount: 0.012,
    haze: ['#8b5cf6', '#3b82f6', '#ec4899', '#ffc206'],
    leaks: [
      ['rgba(255,194,6,0.55)', 'rgba(236,72,153,0.35)'],
      ['rgba(139,92,246,0.5)', 'rgba(34,211,238,0.3)'],
      ['rgba(236,72,153,0.45)', 'rgba(255,194,6,0.35)'],
    ],
  },
  gestures: {
    intro: ['wave', 'point', 'open', 'shrug', 'cheer'],
    speed: ['idle', 'point', 'open', 'cheer', 'point'],
    trust: ['idle', 'open', 'point', 'shrug', 'wave', 'idle', 'thumb'],
    rename: ['open', 'idle', 'point'],
    reach: ['open', 'point', 'idle', 'open', 'idle', 'thumb'],
    ai: ['idle', 'shrug', 'idle', 'open', 'idle', 'thumb'],
    care: ['open', 'idle', 'idle', 'cheer'],
    close: ['cheer', 'open', 'point'],
  },
  intro: 'origin',
  endTagline: 'Free for personal use · macOS',
}
