# Promo videos: details

Architecture, data flows, and decisions behind `tools/demo-video/`. Must-knows are in `CLAUDE.md`; how to regenerate and
edit is in `README.md`.

## Why Remotion

The edit is computed, not hand-placed: cuts snap to the music's bar grid, voice clips and captions come from measured
takes, and footage moments sync to phrases. Remotion makes each frame a pure function of time in React, so that logic is
plain TypeScript (`src/timeline.ts`) and any frame renders alone (`remotion still`), which is what made reviewing
without a full render possible. Its license is free for companies of up to three people (`CREDITS.md`).

## Data flow

1. `scripts/prepare-assets.ts` copies the logo, the dark screenshots, Inter, and each video's track into `public/`, and
   maps the track's strong drum hits (`strongOnsets` in `scripts/lib/audio.ts`) to `public/<id>/music-onsets.json`.
2. `scripts/voice.ts` makes `public/<id>/voice/<section>.wav` plus a manifest: duration, `lead` (silence trimmed off the
   take's start), word-timed caption phrases, and a per-frame loudness envelope.
3. `scripts/footage.ts` transcodes `brand/clip-masters/<footageDir>/*.mov` (ProRes 4444, variable frame rate, 2560
   × 1600) to full-resolution, constant-30-fps H.264 proxies with short GOPs, and writes their manifest: the recording's
   labelled notes become named moments, plus `start`/`end` (usable span) and `key` (key frame).
4. `Root.tsx`'s `calculateMetadata` loads those manifests (every video's footage, since shots can borrow) and calls
   `buildTimeline`, which returns the whole edit as data; the components only render it.

## Timeline rules (`src/timeline.ts`)

- Intro: a 1.9 s logo sting, then the intro voice; `drop1` (the cut into the first beat) lands 0.35 s after the last
  word, and the music is offset so the track's `drop1` lands exactly there. Every later cut sits on that grid.
- Beat sections: the title card owns the first 0.9 s (`LEAD`), the voice follows, the footage gets 0.9 s after it, and
  the end snaps up to a bar (or half a bar with `cutEvery: 0.5`), never shorter than `minBars`.
- The optional lift (video 1's big drop): the voice of the `dropOnPhrase` section splits at the pause before that
  phrase; part A ends 0.3 s before a downbeat, the music jumps forward by whole bars into the last `keepBars` of the
  build so the drop lands on that downbeat, and the phrase starts two beats later. Placing the drop in the pause, not
  under a word, is deliberate: a drop under speech gets ducked into nothing.
- The ending: the close voice ends 0.35 s before the track's final hit; a whole-bar jump (plus `finalJumpNudge`) into
  the track's last section puts the hit there, and `tail` seconds of ring-out end the video.
- Shots: cut points come from `cue` (snapped to the beat just before a phrase), `startsOnDrop`, or weights (snapped to a
  bar). `sync` solves for the clip offset so a labelled moment plays when a phrase starts. `FootageClip` (in
  `scenes/Beat.tsx`) holds the first usable frame when the solved offset is before the usable span and freezes the last
  frame past the recording's end; `pnpm timeline` lists both, so a long hold shows up before any render.

## Music edit decisions

- Grid by measurement, not metadata: ccMixter lists "Snaps" at 145 BPM; onset autocorrelation measures 139.94. A wrong
  tempo makes whole-bar jumps drift audibly.
- Even the measured grid isn't enough: `scripts/align-jump.ts` correlates drum envelopes around each join and found the
  dubstep ending jump 44 ms off and the live-band one 51 ms off. `finalJumpNudge` stores the correction.
- Live drums: "Reach Out" has no stable whole-song beat phase (on-beat and off-beat flip between regions), so its edit
  rests on measured landmarks (band entry 22.73 s, final hit 199.55 s) and crossfaded joins (`joinFade` 0.45 s); "Snaps"
  cuts within one frame.
- Ducking (`audio/Soundtrack.tsx`): speech spans merge across gaps under 0.75 s so the music doesn't pump; the drops get
  "punch windows" that open fully on the hit and ease back under the next line; the final hit plays at `gainFinal`.
  Gains were set from stem renders (`pnpm stems:<id>` + `check-levels`) to keep the mix about 2 dB below full scale.

## Voice decisions

- Per-section takes, so timing stays editable and one bad take doesn't cost the rest.
- `gpt-4o-mini-tts` drops sentences silently (seen on three takes). Each take is transcribed with `whisper-1` and must
  match the script spelling-insensitively ("S three" = "S3", a spoken "dot" vanishes) and end with the last phrase;
  otherwise it's regenerated, up to four times.
- Captions come from the same word timestamps, aligned back to the script's `show` text by edit-distance alignment, so
  captions read as written ("50,000", "getcmdr.com") while timing follows what was said.
- The mouth is the voice's band-limited loudness per frame; the webcam swap reuses the whole pipeline with the takes'
  own audio (`--source=webcam`), and `lead` keeps each take's video in sync with its trimmed audio.

## Look

Two themes in the specs: video 1 is drop-and-glitch (letters slam up, zoom-through cards, white flashes on drops, pulses
on computed downbeats); video 2 is warmer (words step in on eighth notes with a marker swipe, cards slide, no flashes,
amber/rose/teal haze, pulses on the track's real drum hits). Grain, vignette, and light leaks on cuts are shared
(`overlays/Effects.tsx`). The stick figure's lines "boil" (redrawn with a small wobble every three frames), which keeps
it from reading as a vector template.

## Footage caveats that shaped the edit

- The SMB test server's address ("private on 127.0.0.1:11481") shows in the volume picker and copy dialog of the
  `smb`/`hero2` recordings. SMB shots use `closeFraming` to keep the picker out of frame; video 2's close borrows video
  1's `hero` clip because the perspective view can't hide it. The copy dialog in video 2's SMB shot still names it.
- The phone recording has no ADB moment; "Or go over ADB." plays over the finished copy (by decision).
