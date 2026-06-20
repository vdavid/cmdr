# Ido (io) translation style guide

Working notes for translating Cmdr into Ido. Read [`README.md`](../README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice.

## Priority: low (constructed auxiliary language)

Ido (a reformed Esperanto, 1907) is a constructed international auxiliary language with NO native speakers and NO
commercial product localization (no Apple, Microsoft, Spotify, Netflix). The only reference is a partial GNOME Nautilus
catalog (~422 strings, `io/gnome-nautilus/`). This low-priority signal IS the finding: a hobby/community locale, far
below the major locales. Translate only for community goodwill.

## Voice and tone

Friendly, concise, active, calm. Ido is regular and agglutinative-leaning (Esperanto-family); match Cmdr's English voice
directly.

## Formality

**No T-V formality distinction.** One second-person pronoun ("tu"). Use imperative verbs for buttons; in Ido the
imperative ends in `-ez` ("Rinomizez" rename, "Abrogez" cancel, "Serchez" search).

## Decision points

### Script and variants: none to decide

- Latin script with regular spelling, one standardized form. No regional variants, no orthography or gender decision.
  Confidence: high.

### Gender: optional, default neutral

- Ido nouns are gender-neutral by default; gender is marked only by optional affixes (`-ino` feminine, `-ulo` masculine)
  when explicitly needed. Default to the neutral root. Inclusive language is the natural default. Confidence: high.

### Distinct from Esperanto and the other auxlangs

- Ido is its own language (reformed Esperanto); do not mix with Esperanto (eo), Interlingua (ia), or Interlingue (ie)
  catalogs. The `-ez` imperative and forms like "Dokumento" (file) are Ido-specific. Confidence: high.

## Terminology and glossary

Source: GNOME Nautilus (`io/gnome-nautilus/`) only. `tentative` until reviewed. From the catalog: rename → Rinomizez,
trash → Eskombreyo, cancel → Abrogez, search → Serchez, file → Dokumento. Confirm copy (likely "Kopiez"), move (likely
"Movez"), delete (likely "Efacez"), open (likely "Apertez"), folder, settings against the catalog.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `other` (verified `new Intl.PluralRules('io')`, 2026-06-20). Ido plural is regular (`-i`); write
both branches.

## Notes and decisions

- **Length:** comparable to Esperanto/Romance auxlangs. Overflow-check against `en-XA`.
- **Numbers/dates** come from the formatter layer.

## Decisions to confirm with David

- **Ship at all?** Constructed language, no commercial localization, tiny community. Lowest priority; David's call.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/io/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
