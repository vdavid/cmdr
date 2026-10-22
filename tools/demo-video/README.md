# Cmdr demo video

A 90-second promo for Cmdr, built in code with [Remotion](https://www.remotion.dev): a script, a synthesized voice (to
be replaced with David's own), CC BY music cut to the beat, and the app's footage. Change any piece and the edit re-lays
itself out.

Output: `out/cmdr-demo-90s.mp4` (1920 × 1080, 30 fps, H.264 + AAC) and `out/cmdr-demo-90s-poster.png`. Credits and
licenses: `CREDITS.md`.

## Regenerate everything

You need Node (mise), pnpm, ffmpeg, and `secret OPENAI_API_KEY`. Everything under `assets/`, `public/`, and `out/` is
gitignored and comes from these steps.

1. `pnpm install`: a standalone pnpm root (its own `pnpm-workspace.yaml`), separate from the monorepo.
2. Get the music: download the MP3 from the track page in `CREDITS.md` to `assets/music/cdk-snaps.mp3`.
3. `pnpm prepare-assets`: copies the logo, the dark screenshots, Inter, and the track into `public/`.
4. `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice`: one TTS take per script section, checked against the script
   (the model sometimes drops a sentence, so incomplete takes are retried), trimmed, loudness-normalized, transcribed
   for word-timed captions, and turned into a mouth envelope. Takes are cached by text, so rerunning costs only the
   transcription. `--force` regenerates; `--only=trust` does one section.
5. `pnpm footage`: transcodes the ProRes masters listed in `brand/clip-masters/video/video-beats.json` into H.264
   proxies. Beats without footage keep their still placeholders. This is a heavy encode.
6. `pnpm timeline`: prints the computed edit (cuts, voice placement, music jumps, total length).
7. `pnpm studio` to scrub it, `pnpm still --frame=840 out/frame.png` for one frame, and `pnpm render` + `pnpm poster`
   for the final files.
8. Check the mix: `pnpm stems && pnpm check-levels` (voice margin over the music, and the peak).

## Edit it

- **Words**: `src/script.ts`. `say` is what the voice reads, `show` is the caption. Rerun `pnpm voice`.
- **Voice performance**: `VOICE_INSTRUCTIONS` in `src/script.ts`, and `VOICE` in `src/config.ts`.
- **Shots and punch-ins**: each section's `shots` in `src/script.ts` (which footage beat, which still, how the time
  splits, where to zoom).
- **Timing rules** (cut on bar lines, drop placement, the ending): `src/timeline.ts`. Lengths per section: `minBars`.
- **Music**: `MUSIC` in `src/config.ts`. For a new track, find its grid with `node scripts/beat-grid.ts <file>` and its
  drops with `node scripts/analyze-music.ts <file>`, then update the anchors.

## Swap the stick figure for a webcam

The presenter is one swappable layer (`src/presenter/`), so swapping it changes nothing else.

1. Record one take per script section, reading that section's `say` lines: `intro`, `speed`, `trust`, `rename`, `reach`,
   `ai`, `care`, `close`. Framing: head and shoulders, centered, since it shows both full-size (a tall rounded box) and
   in a circle. Leave a breath of silence at the start; it's measured and trimmed.
2. Save them as `public/webcam/<section>.mp4`.
3. `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice --source=webcam`: the takes' audio becomes the voice, and the
   captions and timing follow your real delivery.
4. Set `PRESENTER = { kind: 'webcam' }` in `src/config.ts` and render.

To use your own voice with the stick figure, record audio-only takes as `.mp4` files the same way, run step 3, and keep
`PRESENTER` on `stick`.
