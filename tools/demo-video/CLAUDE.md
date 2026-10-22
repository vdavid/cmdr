# Demo video (Remotion)

Cmdr's 90-second promo, built in code. How to regenerate, edit, and swap in webcam footage: `README.md`. Licenses and
attribution: `CREDITS.md`.

## Module map

- `src/script.ts` is the single source for the words, captions, gestures' phrase order, and shots. `src/config.ts` holds
  the knobs (presenter kind, music anchors and gains, TTS model and voice).
- `src/timeline.ts` lays out everything (pure, deterministic): cuts on bar lines, the big drop on `dropOnPhrase`, the
  ending on the track's final hit. `scripts/timeline.ts` prints it.
- `src/scenes/` (intro, beats, close, sting), `src/presenter/` (the swappable stick figure or webcam slot),
  `src/overlays/` (captions, effects), `src/audio/Soundtrack.tsx` (music segments, ducking, voice).
- `scripts/` produce the gitignored `public/` inputs: `voice.ts`, `footage.ts`, `prepare-assets.ts`, plus the music
  analysis tools `beat-grid.ts` and `analyze-music.ts`, and `check-levels.ts` for the mix.

## Must-knows

- Standalone pnpm root: its own `pnpm-workspace.yaml` (same supply-chain policy as the repo root) and lockfile. Never
  add it to the monorepo workspace.
- Node runs the `.ts` scripts directly (type stripping), so `src/script.ts` and `src/config.ts` stay import-free data.
- Never print or commit `OPENAI_API_KEY`; pass it inline: `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice`.
- `gpt-4o-mini-tts` silently drops sentences sometimes. `voice.ts` transcribes each take and retries incomplete ones;
  keep that check.
- Music jumps must be whole bars of the measured grid (139.94 BPM for "Snaps"), or the edit audibly hiccups.
- The webcam swap keeps video and voice in sync through the manifest's `lead` (the silence trimmed off each take). Don't
  trim the voice files any other way.
- Full renders and `pnpm footage` are CPU-heavy: don't run them while a screen recording is going.
- Remotion's free license covers companies of up to three people (`CREDITS.md`).
