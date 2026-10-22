# Cmdr promo videos

Two 90-second promos for Cmdr, built in code with [Remotion](https://www.remotion.dev): a script, a synthesized voice
(to be replaced with David's own), CC BY music cut to the beat, and the app's footage. Change any piece and the edit
re-lays itself out.

- **`demo`**, composition `CmdrDemo`: "Cmdr in 90 seconds". David's origin story, then speed, undo, bulk rename, reach,
  AI, and care, cut to dubstep. Output `out/cmdr-demo-90s.mp4`.
- **`anywhere`**, composition `CmdrAnywhere`: safe file operations to and from anywhere (SMB, S3, phones, archives and
  restic, transfers, undo), cut to a live indie-rock band with a warmer look. Output `out/cmdr-anywhere-90s.mp4`.

Both are 1920 × 1080, 30 fps, H.264 + AAC, each with a poster PNG beside it. Credits and licenses: `CREDITS.md`.

## Regenerate everything

You need Node (mise), pnpm, ffmpeg, and `secret OPENAI_API_KEY`. Everything under `assets/`, `public/`, and `out/` is
gitignored and comes from these steps. Scripts take `--video=demo` (the default) or `--video=anywhere`.

1. `pnpm install`: a standalone pnpm root (its own `pnpm-workspace.yaml`), separate from the monorepo.
2. Get the music: download each MP3 from its track page in `CREDITS.md` into `assets/music/` under the file name the
   video spec names (`music.source` in `src/videos/<id>.ts`).
3. `pnpm prepare-assets`: copies the logo, the dark screenshots, Inter, and both tracks into `public/`, and maps each
   track's drum hits (the warm theme's pulses follow the real drummer).
4. `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice --video=anywhere`: one TTS take per script section, checked
   against the script (the model sometimes drops a sentence, so incomplete takes are retried), trimmed,
   loudness-normalized, transcribed for word-timed captions, and turned into a mouth envelope. Takes are cached by text,
   so rerunning costs only the transcription. `--force` regenerates; `--only=<section>` does one section.
5. `pnpm footage --video=anywhere`: transcodes the ProRes masters listed in
   `brand/clip-masters/<footageDir>/video-beats.json` (`video` for the first video, `video2` for the second) into H.264
   proxies. Beats without footage keep their still placeholders. This is a heavy encode.
6. `pnpm timeline --video=anywhere`: prints the computed edit (cuts, voice placement, music jumps, which clip span each
   shot plays, total length). Add `--footage-plan` to plan against the masters before transcoding anything.
7. `pnpm studio` to scrub both, then `pnpm render:demo`, `pnpm render:anywhere`, `pnpm poster:demo`, and
   `pnpm poster:anywhere` for the final files.
8. Check the mix: `pnpm stems:anywhere && pnpm check-levels --video=anywhere` (voice margin over the music, and the
   peak).

## Edit it

- **Words, shots, gestures, look, and music edit** of a video: its spec, `src/videos/demo.ts` or
  `src/videos/anywhere.ts`. In the script, `say` is what the voice reads and `show` is the caption; rerun `pnpm voice`
  after changing it. `shots` pick the footage beat, the placeholder still, how the time splits, and where to punch in.
- **A third video**: copy a spec, give it a new `id` and `compositionId`, and add it to `src/videos/index.ts`.
- **Timing rules** (cuts on bar lines, the optional lift, the ending): `src/timeline.ts`. Lengths per section:
  `minBars`.
- **Music**: `music` in the spec. For a new track, find its grid with
  `node scripts/beat-grid.ts <file> <minBpm> <maxBpm> <fromSec> <toSec>` and its sections with
  `node scripts/analyze-music.ts <file>`, then set the anchors. Then run `pnpm align-jump --video=<id>`: live drums
  drift, so it measures how far each whole-bar jump lands off the beat and tells you the `finalJumpNudge` to set.

## Swap the stick figure for a webcam

The presenter is one swappable layer (`src/presenter/`), so swapping it changes nothing else.

1. Record one take per script section, reading that section's `say` lines (the section ids are in the video's spec).
   Framing: head and shoulders, centered, since it shows both full-size (a tall rounded box) and in a circle. Leave a
   breath of silence at the start; it's measured and trimmed.
2. Save them as `public/<videoId>/webcam/<section>.mp4`.
3. `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice --video=<videoId> --source=webcam`: the takes' audio becomes the
   voice, and the captions and timing follow your real delivery.
4. Set `PRESENTER = { kind: 'webcam' }` in `src/config.ts` and render.

To use your own voice with the stick figure, record audio-only takes as `.mp4` files the same way, run step 3, and keep
`PRESENTER` on `stick`.
