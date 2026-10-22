# Credits and licenses

Everything third-party in the demo video, with the evidence we checked. The heavy files themselves live in the
gitignored `assets/` and `public/`.

## Music

**"Snaps (Original RumbleStep Mix)" by cdk**

- Track page: https://ccmixter.org/files/cdk/47643
- Artist: cdk (https://ccmixter.org/people/cdk, www.cdk.me)
- License: Creative Commons Attribution 3.0 (CC BY 3.0), http://creativecommons.org/licenses/by/3.0/. The track page
  states "2014 - Licensed under Creative Commons Attribution (3.0)". CC BY allows commercial use, including a promo for
  a commercial product, as long as we credit the artist.
- Released: "uploaded: Mon, Oct 6, 2014 @ 10:34 AM" in the track page header, so it predates generative music tools.
- Checked on 2026-10-22 by loading the track page live.
- Length 3:17. ccMixter lists 145 BPM; `scripts/beat-grid.ts` measures 139.94 BPM, which is what the edit uses.
- Downloaded MP3 (320 kbps): `assets/music/cdk-snaps.mp3`. The track page also offers an optional paid "CC+" license; we
  don't need it, because CC BY 3.0 already covers commercial use with attribution.
- How we edited it (CC BY allows adaptations; say so in the credit): trimmed, two whole-bar jumps, and volume ducking
  under the voice. See `src/timeline.ts`.

**Required attribution**, wherever the video is published (video description, or an end-credit line):

> Music: "Snaps (Original RumbleStep Mix)" by cdk (https://ccmixter.org/files/cdk/47643), licensed under CC BY 3.0
> (https://creativecommons.org/licenses/by/3.0/). Edited for length.

Runner-ups the music search found (same license and site, both downloaded to `assets/music/`, neither used):

- "Warriors Fall (Original RumbleStep Mix)" by cdk feat. Darryl J, https://ccmixter.org/files/cdk/50691, CC BY 3.0,
  uploaded 2015-06-27. Credit both artists if used.
- "Drive" by Alex Beroza feat. cdk & Darryl J, https://ccmixter.org/files/AlexBeroza/43098, CC BY 3.0, uploaded
  2013-07-28. Credit all three if used.

## Voice

Synthesized with OpenAI's text-to-speech API: model `gpt-4o-mini-tts`, voice `cedar`, steered by `VOICE_INSTRUCTIONS` in
`src/script.ts`. Caption timing comes from OpenAI's `whisper-1` word timestamps.

- OpenAI's usage policies require telling viewers that a TTS voice is AI-generated, not a human:
  https://developers.openai.com/api/docs/guides/text-to-speech. If this version is ever published with the synthetic
  voice, add a line like "Voice is AI-generated" to the video or its description. Replacing it with David's own recorded
  voice (see `README.md`) removes the need.
- Heads-up: OpenAI has deprecated `gpt-4o-mini-tts` and shuts it down on 2027-01-06. Its successor only runs through the
  Realtime API. Regenerate before then or switch `scripts/voice.ts` over (checked 2026-10-22 on the TTS guide).

## Rendering

[Remotion](https://www.remotion.dev) 4.0.536. Remotion is free for individuals and for companies of up to three people,
commercial use included. Bigger companies need a Company License: https://www.remotion.dev/docs/license/pricing (checked
2026-10-22). Rymdskottkärra AB is within the free tier; revisit if the team grows past three.

## Brand assets and type

- Logo `brand/logos/cmdr.svg` and the product screenshots in `brand/screenshots/` are Cmdr's own.
- Inter (the website's font, `apps/website/public/fonts/`) is licensed under the SIL Open Font License 1.1.
