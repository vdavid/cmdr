# <Language name> (<tag>) translation style guide

Working notes for translating Cmdr into this language. Copy this `_template/` folder to `<tag>/` (it has `style.md` and
`glossary.md`) and fill every section before the first translation pass. Read [`README.md`](../README.md) for how this
fits the translation process.

This is a living doc, and capturing is your job, not optional. Whenever you discover a convention, gotcha, decision
point, or rule that wasn't already written where you looked for it, write it down: per-language findings go here; a
cross-language rule that's missing (like an ICU mechanic) goes in the process guide or this template so the next
translator inherits it instead of rediscovering it.

## Voice and tone

How Cmdr should sound in this language. Cmdr's English voice is friendly, concise, active, and never alarmist (error
messages stay calm and actionable, and avoid the words "error" and "failed"). State the equivalent register for this
language.

## Formality

The form of address to use (formal vs informal second person, where the language distinguishes them), and any
conventions for imperatives in UI actions (buttons, menu items).

## Decision points

The localization choices a translator or the project owner must make for this language, beyond formality above. Research
each, including how the majors handle it both in their products and on their sites (Apple, Microsoft, Google, Spotify,
Netflix, etc.), and a recommended default; flag the ones only David can settle. Look for, at least: script (Cyrillic vs
Latin, Simplified vs Traditional, Perso-Arabic vs Devanagari, etc.), which regional variant to target and how the
variants differ, gender and inclusive-language handling (gendered grammar, neutral forms), and any other tricky part of
localizing to this language (honorifics, word order, capitalization, RTL, length). One short block per decision point:
the choice, the options, concrete majors-examples, the recommendation, and a confidence.

## Terminology and glossary

The agreed translation for recurring product and file-manager terms (for example: pane, tab, volume, listing, transfer,
trash, viewer). One row per term so the whole catalog stays consistent. Add terms as they come up.

| English term | This language | Notes |
| ------------ | ------------- | ----- |

## Brand and do-not-translate

Keep these verbatim (they are product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri,
Rust, Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the
`desktop-i18n-dont-translate` check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

This language's CLDR plural categories (run `new Intl.PluralRules('<tag>').resolvedOptions().pluralCategories`), and any
grammar notes a translator needs (gender or case agreement that interacts with counts). The `desktop-i18n-plural` check
requires every plural message to cover the categories this language needs.

## Notes and decisions

Anything else: punctuation conventions, quotation marks, number and date phrasing peculiar to this language, and any
case-by-case rulings made during translation so they are not relitigated.

- **ICU mechanics** (catalog-level, not language-specific, but easy to miss when handed only "translate these"): double
  every apostrophe in a value (`'` becomes `''`; ICU treats a lone `'` as an escape and silently swallows text), and
  keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/<tag>/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
