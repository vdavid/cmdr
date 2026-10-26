// Video 2, "Cmdr, anywhere": safe file operations to and from everything that stores files, cut to a live indie-rock
// band. Pure data (type-only imports), so Node scripts can load it with type stripping.
import type { Section, Shot, VideoSpec } from './types.ts'

/** A shot on this video's recordings, with the main-window still as its placeholder. */
const shot = (beat: string, weight: number, focus: Shot['focus'], rest: Partial<Shot> = {}): Shot => ({
  beat,
  still: 'app-main-dark.webp',
  weight,
  focus,
  ...rest,
})

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
    // The share lists 5,000 photos on "way faster"; the copy within the share lands on "right on the server". The
    // shots hold a light crop to the bottom left: the volume picker at the top shows the test server's address (see
    // video-beats.json), and `closeFraming` keeps it cropped off inside the window for the whole shot, while the file
    // names and the "5,000 files" count stay in frame.
    shots: [
      shot(
        'smb',
        0.55,
        { x: 0.1, y: 0.95, zoom: 1.18 },
        { closeFraming: true, sync: { moment: 'listed: 5,000 files', phrase: 1 } },
      ),
      shot(
        'smb',
        0.45,
        { x: 0.1, y: 0.95, zoom: 1.18 },
        { closeFraming: true, cue: 2, sync: { moment: 'F5: copy "Lisbon trip"', phrase: 2 } },
      ),
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
    // One continuous shot in the R2 bucket: F5 and the cost estimate ("About $0.01") arrive a tenth of a second apart,
    // so the estimate (the key frame) lands on "tells you what it'll cost" and the bucket view opens the shot.
    shots: [shot('s3', 1, { x: 0.5, y: 0.6, zoom: 1.4 }, { sync: { moment: 'key', phrase: 1 } })],
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
    // Browsing the phone's camera roll, then the copy off it runs under "four times faster". The recording has no ADB
    // moment, so "Or go over ADB" plays over the finished copy.
    shots: [
      shot('phone', 0.4, { x: 0.5, y: 0.55, zoom: 1.15 }, { sync: { moment: 'DCIM/Camera', phrase: 1 } }),
      shot(
        'phone',
        0.6,
        { x: 0.5, y: 0.55, zoom: 1.25 },
        { cue: 2, sync: { moment: 'all selected; copy', phrase: 2 } },
      ),
    ],
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
    // A zip opens like a folder; then the restic repo, unlocked into snapshots, and a file restored, each on its words.
    shots: [
      shot('archives', 0.35, { x: 0.5, y: 0.55, zoom: 1.15 }, { sync: { moment: 'Enter on a zip', phrase: 0 } }),
      shot(
        'archives',
        0.15,
        { x: 0.5, y: 0.5, zoom: 1.2 },
        { cue: 1, sync: { moment: 'a restic backup repo', phrase: 1 } },
      ),
      shot('archives', 0.25, { x: 0.5, y: 0.55, zoom: 1.2 }, { cue: 2, sync: { moment: 'unlocked', phrase: 2 } }),
      shot('archives', 0.25, { x: 0.5, y: 0.55, zoom: 1.25 }, { cue: 3, sync: { moment: 'restored', phrase: 3 } }),
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
    // A copy with live progress and ETA; Pause and Resume on "Pause, resume"; the Operations window and the queue on
    // "Queue up more, and send them to the background".
    shots: [
      shot(
        'transfers',
        0.4,
        { x: 0.5, y: 0.55, zoom: 1.15 },
        { sync: { moment: 'F5: copy "Annual report"', phrase: 0 } },
      ),
      shot(
        'transfers',
        0.3,
        { x: 0.5, y: 0.55, zoom: 1.25 },
        { cue: 2, sync: { moment: 'Pause the running copy', phrase: 2 } },
      ),
      shot('transfers', 0.3, { x: 0.5, y: 0.5, zoom: 1.15 }, { cue: 3, sync: { moment: 'Background', phrase: 4 } }),
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
    // The copy disappears on "Undo it."; the operation log rolls back an older copy on "roll it back".
    shots: [
      shot('undo', 0.45, { x: 0.5, y: 0.55, zoom: 1.2 }, { sync: { moment: 'undone', phrase: 1 } }),
      shot(
        'undo',
        0.55,
        { x: 0.5, y: 0.5, zoom: 1.2 },
        { cue: 2, sync: { moment: 'Roll back the earlier copy', phrase: 2 } },
      ),
    ],
  },
  {
    id: 'close',
    kind: 'close',
    phrases: [
      { say: 'Safe file operations, to and from anywhere.', show: 'Safe file operations, to and from anywhere.' },
      { say: 'Commander.', show: 'Cmdr.' },
      { say: 'Free for personal use, at get commander dot com.', show: 'Free for personal use, at getcmdr.com' },
    ],
    // The first video's hero clip: hero2's NAS pane shows the SMB test server's address ("private on 127.0.0.1:11481"),
    // which no punch-in can hide in this perspective view. Switch back to `{ beat: 'hero2' }` once it's re-recorded.
    shots: [{ beat: 'hero', source: 'demo', still: 'app-main-dark.webp', weight: 1 }],
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
    glowPulse: 0.7,
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
