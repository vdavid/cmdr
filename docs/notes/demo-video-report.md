# Promo videos: report (2026-10-23)

Two 90-second Cmdr promos, built in code with Remotion in `tools/demo-video/`, rendered from the real app footage that
`pnpm marketing:clips --video` recorded (`demo-video-footage.md`). For David's private review; nothing is published. How
the tool works: `tools/demo-video/CLAUDE.md` and its `DETAILS.md`.

## The outputs

All files are gitignored, in `tools/demo-video/out/`.

- `cmdr-demo-90s.mp4`: "Cmdr in 90 seconds". 89.0 s, 1920 × 1080, 30 fps, H.264 + AAC, 70 MB.
- `cmdr-anywhere-90s.mp4`: "Cmdr, anywhere" (safe file operations to and from anywhere). 89.0 s, same format, 74 MB.
- `cmdr-demo-90s-web.mp4`, `cmdr-anywhere-90s-web.mp4`: web copies, 16 MB each (two-pass H.264 at 1.3 Mbps, faststart;
  small UI text stays crisp).
- `cmdr-demo-90s-poster.png`, `cmdr-anywhere-90s-poster.png`: the end card at 88 s.

## What's in them

**Video 1, `demo`.** Logo sting; the stick figure tells the origin story (25 years of Total Commander, a Mac with
nothing like it) beside a kinetic panel; then six beats with slam-in title cards: speed (a 50,000-file folder, search),
undo (copy, pause, resume, and a rollback that walks the bar back to zero), bulk rename with its live preview, reach
(the NAS and the phone from video 2's recordings, a zip, a Git repo as folders), AI (the local model turning "tax report
from last year" into last year's tax files), and care (15 languages previewing live, light mode, the keyboard). Close:
the figure full size beside the app in perspective, then the end card on the music's last hit.

**Video 2, `anywhere`.** The intro builds a ring of places (Mac, NAS, Phone, Cloud, Backups) as they're named, then
drops Cmdr in the middle with files flowing to and from each. Beats: SMB (5,000 photos on the NAS, a copy within the
share that happens on the server), S3 (an upload to a real Cloudflare R2 bucket with "About $0.01 at Cloudflare R2 list
prices"), phone (camera roll, a 323 MB/s copy off a Pixel 8), archives (a zip, a restic repo unlocked into snapshots, a
file restored), transfers (live progress and ETA, pause and resume, the Operations window and its queue), and undo
(Undo, then the operation log's Roll back). A warmer look to match its music: words step in on eighth notes with a
mustard marker swipe, cards slide, no white flashes, amber/rose/teal haze, and pulses that follow the real drummer.

**In both:** the app window sits still with a soft Cmdr-gold glow that swells gently on the beat, and punch-ins ease in
slowly on the moment that matters, always keeping the whole window on screen. Every recorded moment is synced to its
spoken line (for example, the rollback cuts in on the big drop and "the bar walks back to zero" lands on "watch every
file go back"); cuts sit on the music's bar lines; burned-in captions light up word by word; and the stick figure
lip-syncs to the voice, gestures per phrase, and shrinks into a corner "webcam bubble" during the beats.

## Review

Checked on the final renders, not the previews.

- **Frames:** two contact sheets per video (16 frames each) at every cut and synced moment, plus full-size stills of
  each synced moment while planning. Every moment lands on its line; the only untidy frames are mid-morph transitions
  (the presenter shrinking into the bubble as a title card arrives), which pass in about half a second.
- **Audio:** stems rendered separately and compared (`pnpm stems:<id>` + `pnpm check-levels --video=<id>`).
  - Video 1: −15.8 LUFS integrated, true peak −2.9 dBFS. While someone talks, the voice sits a median 14.9 dB above the
    music, and 95 % of speech windows clear it by 4.2 dB or more; the few below are single syllables at word edges.
  - Video 2: −17.0 LUFS, peak −3.5 dBFS, voice margin median 14.7 dB, 5th percentile 4.8 dB, worst +1.1 dB.
  - No clipping anywhere. The music opens fully on each drop and ducks under every line.

Fixes made during review: the beat label is a frosted, near-opaque pill that wipes in and out (a fade let app rows show
through it mid-transition during punch-ins), checked at two frames in each beat's first 2 s of both final renders; SMB
shots start zoomed in so the test server's address stays out of frame (video 1's NAS slice now matches video 2's
framing); video 2's S3 beat became one continuous shot so the cost estimate lands on its line; the level check had
measured the peak on a mono downmix (3 dB too high) and now reads every channel.

Fixes after David's first look (2026-10-26):

- **The window no longer pulses.** It used to scale on every beat; now only its gold glow swells.
- **No more half-off-screen windows.** The punch-ins scaled the whole window toward their focus with no bounds, so the
  1.3–1.4× zooms pushed it off the top of the canvas (video 2's SMB and S3 shots and most low-focus shots; video 1's NAS
  and "Made with care" shots), and the close's turning app ran off the right edge in both videos. Punch-ins now grow the
  window only within the canvas and crop inside it beyond that, and the close's card is smaller and turns less. A
  frame-by-frame check in the timeline build fails any shot that would put the window off-canvas
  (`tools/demo-video/DETAILS.md`, "The app window").
- Checked on stills at every shot's start, middle, and end in both videos, before and after.

## For David to judge

- **The music, by ear.** I chose both tracks from their pages, tags, and loudness analysis without hearing them. Snaps
  (video 1) is instrumental dubstep with a sharp drop; Reach Out (video 2) is guitars, bass, and live drums. The edits
  jump within each track by whole bars, aligned by measurement (`pnpm align-jump`), so they should be seamless, but only
  a listen confirms it: the joins are at 24.5 s and 69.1 s in video 1, and 69.3 s in video 2.
- **The voice is synthetic** (OpenAI `gpt-4o-mini-tts`, voice `cedar`). OpenAI's usage policy requires telling viewers a
  voice is AI-generated, so publishing this version needs a line like "Voice is AI-generated"; recording your own voice
  removes that. The model shuts down on 2027-01-06 (its successor is Realtime-only), so regenerate before then if
  needed.
- **The SMB test server's address** ("private on 127.0.0.1:11481") is visible in video 2's in-share copy dialog (about
  19–21 s). The recordings' NAS-name fix didn't work; a re-record with a named share would remove it. Video 2's close
  borrows video 1's hero clip for the same reason.
- **"Or go over ADB."** plays over the finished phone copy: the recording has no ADB moment (kept by decision).
- **Pronunciation:** the voice says "Commander" for Cmdr, and spells out S M B, A W S, A D B, and E T As.

## Regenerate or change them

`tools/demo-video/README.md` has the full steps. In short: `pnpm install`, download the two tracks (`CREDITS.md`),
`pnpm prepare-assets`, `pnpm voice --video=<id>` (with `OPENAI_API_KEY=$(secret OPENAI_API_KEY)`),
`pnpm footage --video=<id>` for both videos (video 1 borrows video 2's clips), then `pnpm render:<id>`,
`pnpm poster:<id>`, and `pnpm web-copy --video=<id>`. A render takes about 3 minutes; transcoding all footage about 3
more.

Everything video-specific is one file per video, `tools/demo-video/src/videos/demo.ts` and `anywhere.ts`: the script
(what the voice says and what the caption shows), which recorded moment syncs to which phrase, the punch-ins, gestures,
music anchors, and the look. Change it, rerun `pnpm voice` if the words changed, and `pnpm timeline --video=<id>` prints
the new edit before any render.

## Swap in real webcam footage

The presenter is one layer; nothing else changes.

1. Record one take per script section, reading its lines (`intro`, the beats, `close`; the ids are in each video's
   spec). Frame head and shoulders, centered: it appears both full-size (a tall rounded box) and in a circle. Leave a
   breath of silence at the start; it's measured and trimmed.
2. Save them as `tools/demo-video/public/<video>/webcam/<section>.mp4`.
3. Run `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice --video=<video> --source=webcam`: your takes' audio becomes
   the voice, and the captions, cuts, and music edit follow your real delivery.
4. Set `PRESENTER = { kind: 'webcam' }` in `src/config.ts` and render.

For your voice with the stick figure, record audio-only takes the same way, run step 3, and leave `PRESENTER` on the
stick figure.

## Credits

Full evidence and the attribution text for video descriptions: `tools/demo-video/CREDITS.md`.

- **Video 1 music:** "Snaps (Original RumbleStep Mix)" by cdk, https://ccmixter.org/files/cdk/47643, CC BY 3.0, uploaded
  2014-10-06.
- **Video 2 music:** "Reach Out (Stems)" by spinningmerkaba, https://ccmixter.org/files/jlbrock44/32919, CC BY 3.0,
  uploaded 2011-08-07.
- Both are credited on the end card. CC BY also needs the full credit wherever the video is posted (the text is in
  `CREDITS.md`).
- **Voice:** OpenAI TTS (see above). **Rendering:** Remotion 4.0.536, free for companies of up to three people.
  **Type:** Inter (SIL OFL 1.1). Logo and footage are Cmdr's own.
