# Promo videos (Remotion)

Cmdr's 90-second promos (`demo` and `anywhere`), built in code. How to regenerate, edit, add a video, and swap in webcam
footage: `README.md`. Licenses and attribution: `CREDITS.md`.

## Module map

- `src/videos/<id>.ts` is everything video-specific: script, captions, shots, gestures, voice direction, music edit, and
  theme (types in `src/videos/types.ts`, list in `src/videos/index.ts`). `src/config.ts` holds shared knobs (presenter
  kind, TTS model, `public/` paths).
- `src/timeline.ts` lays out a video from its spec (pure, deterministic): cuts on bar lines, the optional lift on
  `dropOnPhrase`, the ending on the track's final hit. `pnpm timeline --video=<id>` prints it. `src/lib/framing.ts`
  places the app window and its punch-ins, and the timeline build throws if any frame puts the window off-canvas.
- `src/scenes/`, `src/presenter/` (the swappable stick figure or webcam slot), `src/overlays/`, and
  `src/audio/Soundtrack.tsx` are shared machinery, themed by the spec.
- `scripts/` produce the gitignored `public/<id>/` inputs (`voice.ts`, `footage.ts`, `prepare-assets.ts`) and analyze
  music and mixes (`beat-grid.ts`, `analyze-music.ts`, `align-jump.ts`, `check-levels.ts`); all take `--video=<id>`.

## Must-knows

- Standalone pnpm root: its own `pnpm-workspace.yaml` (same supply-chain policy as the repo root) and lockfile. Never
  add it to the monorepo workspace.
- Node runs the `.ts` scripts directly (type stripping), so `src/videos/*.ts` and `src/config.ts` stay data with
  type-only imports.
- Never print or commit `OPENAI_API_KEY`; pass it inline: `OPENAI_API_KEY=$(secret OPENAI_API_KEY) pnpm voice`.
- `gpt-4o-mini-tts` silently drops sentences sometimes. `voice.ts` transcribes each take and retries incomplete ones;
  keep that check. It compares spelling-insensitively ("S three" vs "S3"), or good takes fail.
- Music jumps are whole bars of the measured grid plus a measured nudge (`pnpm align-jump`); live drums drift, and even
  the dubstep jump was 44 ms off by grid alone. Rerun it after any change that moves the ending.
- The webcam swap keeps video and voice in sync through the manifest's `lead` (the silence trimmed off each take). Don't
  trim the voice files any other way.
- Full renders and `pnpm footage` are CPU-heavy: don't run them while a screen recording is going.
- Remotion's free license covers companies of up to three people (`CREDITS.md`).

Architecture, timeline rules, and the decisions behind the music, voice, and footage edits: `DETAILS.md`.
