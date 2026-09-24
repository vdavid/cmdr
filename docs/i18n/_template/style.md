# <Language name> (<tag>) translation style guide

Working notes for translating Cmdr into this language. Copy this `_template/` folder to `<tag>/` (it has `style.md`, an
empty `terms.json`, and the `decisions.md` and `review-queue.md` stubs) and fill every section before the first
translation pass. Read `../README.md` for how this fits the translation process.

This is a living doc, and capturing is your job, not optional. Whenever you discover a convention, gotcha, decision
point, or rule that wasn't already written where you looked for it, write it down: per-language findings go here; a
cross-language rule that's missing (like an ICU mechanic) goes in the process guide or this template so the next
translator inherits it instead of rediscovering it.

## Digest

The must-know rules for this language, in ~1,500–2,000 tokens: formality and address, voice, capitalization, punctuation
(quotes, ellipsis, spacing), brand inflection, plural notes, and the top traps. `pnpm i18n:brief` copies this section
into every brief verbatim, up to the next `##` heading, so it's what a translator reads first. It SUMMARIZES the
sections below; keep the two in sync whenever you change either.

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

## Terminology

The agreed translation for recurring product and file-manager terms (pane, tab, volume, listing, transfer, trash,
viewer) lives in `terms.json`, one ruling per concept from the shared `../concepts.json`: `chosen`, `sources`,
`confidence`, plus `accept` / `forms` / `avoid` / `exceptions` as needed. Schema: `../termbase.md`. Source every ruling
from the reference pile (`_ignored/i18n/<tag>/` in the main clone; recipes in `../reference-pile/how-to-mine.md`), never
guess, and add it as you settle it. Here, record only what cuts across terms (how this language compounds, where the
article goes in a short label).

## Brand and do-not-translate

Keep these verbatim (they are product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri,
Rust, Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the
`desktop-i18n-dont-translate` check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

This language's CLDR plural categories (run `new Intl.PluralRules('<tag>').resolvedOptions().pluralCategories`), and any
grammar notes a translator needs (gender or case agreement that interacts with counts). The `desktop-i18n-plural` check
requires every plural message to cover the categories this language needs.

## Notes and decisions

Anything else: punctuation conventions, quotation marks, and number and date phrasing peculiar to this language.
Case-by-case rulings made during translation go in `decisions.md` under a heading citing their keys, so the brief can
surface them and nobody relitigates them.

- **ICU mechanics** (catalog-level, not language-specific, but easy to miss when handed only "translate these"): double
  every apostrophe in a value (`'` becomes `''`; ICU treats a lone `'` as an escape and silently swallows text), and
  keep every `{placeholder}` and `<tag>` verbatim. Full rules: `docs/i18n/translator-instructions.md` (embedded in every
  brief) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
