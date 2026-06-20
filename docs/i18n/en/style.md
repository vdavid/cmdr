# English (en) translation style guide

English is Cmdr's **source language**: the catalog under `messages/en/` is authored, not translated, and `en` is the
final fallback for every other locale (see [`i18n.md`](../../guides/i18n.md) § Locale-format convention). So this file
is a short note, not a translation brief, and it has no glossary or formality call to make.

## Voice and tone

The canonical voice lives in the writing-style guide, not here: [`docs/style-guide.md`](../../style-guide.md) (read it
before writing any user-facing string). The essentials every translator should know about the source they're working
from: friendly, concise, active voice, sentence case for every title and label, no "just/simple/easy", and error and
crash copy that stays calm and actionable and never uses the words "error" or "failed". The app may speak as David where
deliberately personal (onboarding, About); the website speaks product-first.

## Formality

English has no T-V distinction, so the formality choice every other language must make doesn't exist here. The English
"you" is register-neutral; how warm or formal the copy feels comes from word choice and the style guide, not a pronoun.
This is exactly why each target-language style guide must settle formality explicitly: the source can't encode it.

## Decision points

There's essentially nothing to decide for `en` as a localization target, but two source-side facts shape every
downstream translation:

- **`en` is the reference English, written in a region-neutral register.** It isn't tagged `en-US` or `en-GB`. Spelling
  leans American ("color", "canceled", "behavior") because that's the larger macOS audience and Apple's default English,
  but the copy avoids region-loud idioms. No `en-GB` / `en-AU` variant is planned; add one only if British/Australian
  spelling ("colour", "cancelled") becomes a real ask. Confidence: high.
- **Gender-neutral by construction.** Source copy already uses "they/them" and avoids gendered nouns, so translators
  inherit gender-neutral intent. When a target language forces grammatical gender on a word the English left neutral,
  that's a target-language decision point (see each language's file), not something the source resolves.

## Terminology and glossary

The English terms ARE the glossary every other language maps from: pane, tab, volume, listing, transfer, trash, viewer,
and so on. There's nothing to translate, so no table here. Each target-language guide owns the mapping from these terms
into its language.

## Brand and do-not-translate

Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens, are verbatim
in every language including the source. Enforced by `desktop-i18n-dont-translate`; list in
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

English CLDR categories: `one`, `other`. The source author writes both branches for any counted string; that's the
template each translation re-expresses in its own categories (which may be more: see `cy`'s six).

## Notes and decisions

- Roster: the source is region-neutral en; a British/Australian variant (en-GB) is a wave-2 follow-on (mainly Trash->Bin
  and -our/-ise spelling). See [`language-selection-decisions.md`](../language-selection-decisions.md).
- **Ellipsis**: the catalog uses three literal ASCII dots ("Sending...") rather than the single `…` character; several
  translations match this shape deliberately. This is a source convention every locale inherits.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/en/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
