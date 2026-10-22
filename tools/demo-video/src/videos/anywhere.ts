// Video 2, "Cmdr, anywhere": safe file operations to and from everything that stores files, cut to a live indie-rock
// band. Pure data (type-only imports), so Node scripts can load it with type stripping.
import type { Section, VideoSpec } from './types.ts'

const script: Section[] = [
  {
    id: 'intro',
    kind: 'intro',
    phrases: [
      { say: 'Your files live everywhere.', show: 'Your files live everywhere.' },
      {
        say: 'Your Mac, your NAS, your phone, the cloud, your backups.',
        show: 'Your Mac, your NAS, your phone, the cloud, your backups.',
      },
      { say: 'Commander moves them safely between all of them.', show: 'Cmdr moves them safely between all of them.' },
    ],
    shots: [],
  },
  {
    id: 'smb',
    kind: 'beat',
    minBars: 4.5,
    title: 'Your NAS, faster.',
    phrases: [
      {
        say: "Network shares run on Commander's own S M B engine.",
        show: "Network shares run on Cmdr's own SMB engine.",
      },
      { say: "It's way faster than the Mac's built-in one,", show: "It's way faster than the Mac's built-in one," },
      {
        say: 'and copies within a share happen right on the server.',
        show: 'and copies within a share happen right on the server.',
      },
    ],
    shots: [
      { beat: 'smb', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.25, y: 0.16, zoom: 1.4 } },
      {
        beat: 'smb',
        still: 'app-main-dark.webp',
        weight: 0.5,
        focus: { x: 0.5, y: 0.6, zoom: 1.15 },
        moment: 'serverCopy',
      },
    ],
  },
  {
    id: 's3',
    kind: 'beat',
    minBars: 4,
    title: 'Buckets, with a price tag.',
    phrases: [
      { say: 'S three buckets on A W S, R two, B two, and more.', show: 'S3 buckets on AWS, R2, B2, and more.' },
      {
        say: 'Before you copy, Commander tells you what it will cost.',
        show: "Before you copy, Cmdr tells you what it'll cost.",
      },
    ],
    shots: [
      { beat: 's3', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.7, y: 0.4, zoom: 1.3 } },
      { beat: 's3', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.5, y: 0.5, zoom: 1.2 }, moment: 'cost' },
    ],
  },
  {
    id: 'phone',
    kind: 'beat',
    minBars: 4.5,
    title: 'Your phone, plugged in.',
    phrases: [
      { say: 'Plug in your Android phone.', show: 'Plug in your Android phone.' },
      { say: 'It just works, no app needed,', show: 'It just works, no app needed,' },
      {
        say: "and it's up to four times faster than Android File Transfer.",
        show: "and it's up to four times faster than Android File Transfer.",
      },
      { say: 'Or go over A D B.', show: 'Or go over ADB.' },
    ],
    shots: [{ beat: 'phone', still: 'app-main-dark.webp', weight: 1, focus: { x: 0.3, y: 0.45, zoom: 1.3 } }],
  },
  {
    id: 'archives',
    kind: 'beat',
    minBars: 4.5,
    title: 'Archives are folders.',
    phrases: [
      { say: 'Zip, tar, and seven-zip files open like folders.', show: 'Zip, tar, and 7z files open like folders.' },
      { say: 'Even restic backups:', show: 'Even restic backups:' },
      { say: 'unlock once, open any snapshot,', show: 'unlock once, open any snapshot,' },
      { say: 'copy your files back.', show: 'copy your files back.' },
    ],
    shots: [
      { beat: 'archives', still: 'app-main-dark.webp', weight: 0.5, focus: { x: 0.3, y: 0.5, zoom: 1.25 } },
      {
        beat: 'archives',
        still: 'app-main-dark.webp',
        weight: 0.5,
        focus: { x: 0.7, y: 0.5, zoom: 1.3 },
        moment: 'restic',
      },
    ],
  },
  {
    id: 'transfers',
    kind: 'beat',
    minBars: 4.5,
    title: 'Honest transfers.',
    phrases: [
      { say: 'Every transfer is honest,', show: 'Every transfer is honest,' },
      { say: 'with real progress and real E T As.', show: 'with real progress and real ETAs.' },
      { say: 'Pause, resume, or cancel.', show: 'Pause, resume, or cancel.' },
      { say: 'Queue up more,', show: 'Queue up more,' },
      { say: 'and send them to the background.', show: 'and send them to the background.' },
    ],
    shots: [
      { beat: 'transfers', still: 'app-main-dark.webp', weight: 0.55, focus: { x: 0.5, y: 0.55, zoom: 1.2 } },
      {
        beat: 'transfers',
        still: 'app-main-dark.webp',
        weight: 0.45,
        focus: { x: 0.5, y: 0.85, zoom: 1.35 },
        moment: 'queue',
      },
    ],
  },
  {
    id: 'undo',
    kind: 'beat',
    minBars: 4,
    title: 'Undo it.',
    phrases: [
      { say: 'Changed your mind?', show: 'Changed your mind?' },
      { say: 'Undo it.', show: 'Undo it.' },
      { say: 'Or roll it back, even days later.', show: 'Or roll it back, even days later.' },
    ],
    shots: [{ beat: 'undo', still: 'app-main-dark.webp', weight: 1, focus: { x: 0.68, y: 0.6, zoom: 1.35 } }],
  },
  {
    id: 'close',
    kind: 'close',
    phrases: [
      { say: 'Safe file operations, to and from anywhere.', show: 'Safe file operations, to and from anywhere.' },
      { say: 'Commander.', show: 'Cmdr.' },
      { say: 'Free for personal use, at get commander dot com.', show: 'Free for personal use, at getcmdr.com' },
    ],
    shots: [{ beat: 'hero2', still: 'app-main-dark.webp', weight: 1 }],
  },
]

export const anywhere: VideoSpec = {
  id: 'anywhere',
  compositionId: 'CmdrAnywhere',
  footageDir: 'video2',
  script,
  voiceInstructions: [
    'Voice: a warm, natural man in his early forties, a developer showing a friend something he built and is proud of.',
    'Tone: relaxed, sunny, quietly confident, with a smile in the voice. Never salesy, never announcer-like.',
    'Pacing: an easy, rhythmic groove with short natural pauses between sentences, like talking over a good song.',
    'Pronunciation: "Commander" is the product name, said as one ordinary word. "NAS" rhymes with "jazz".',
    '"restic" is said "RES-tick". Letters in "S M B", "A W S", "A D B", and "E T As" are spelled out.',
  ].join(' '),
  music: {
    source: 'spinningmerkaba-reach-out.mp3',
    credit: 'Music: "Reach Out" by spinningmerkaba · CC BY 3.0',
    bpm: 112.94,
    cutEvery: 0.5,
    // The first full-band downbeat after the drum fill.
    drop1: 22.73,
    finalSection: 143.0,
    finalHit: 199.55,
    tail: 2.2,
    joinFade: 0.45,
    finalJumpNudge: -0.051,
    gainUnderVoice: 0.19,
    gainOpen: 0.66,
    gainFinal: 0.78,
  },
  theme: {
    titleStyle: 'groove',
    transition: 'slide',
    flashes: false,
    pulse: 'onsets',
    pulseAmount: 0.007,
    haze: ['#f59e0b', '#f43f5e', '#14b8a6', '#ffc206'],
    leaks: [
      ['rgba(255,170,60,0.55)', 'rgba(244,63,94,0.3)'],
      ['rgba(255,194,6,0.5)', 'rgba(20,184,166,0.25)'],
      ['rgba(251,113,133,0.45)', 'rgba(255,194,6,0.35)'],
    ],
  },
  gestures: {
    intro: ['open', 'point', 'cheer'],
    smb: ['point', 'idle', 'open'],
    s3: ['open', 'point'],
    phone: ['point', 'thumb', 'open', 'idle'],
    archives: ['open', 'idle', 'point', 'thumb'],
    transfers: ['open', 'idle', 'point', 'idle', 'wave'],
    undo: ['shrug', 'thumb', 'open'],
    close: ['open', 'cheer', 'point'],
  },
  intro: 'everywhere',
  endTagline: 'Safe file operations, to and from anywhere',
}
