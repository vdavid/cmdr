# Filipino (fil) translation style guide

Working notes for translating Cmdr into Filipino (Tagalog-based). Read `../README.md` for how this fits the translation
process.

This is the language base (`fil`), the universal Filipino set. Filipino is the standardized, Tagalog-based national
language of the Philippines (the pile's folder is `fil-PH`; the base `fil` is the right tag here since there's one
standard).

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Microsoft's Filipino
style guide targets warm, relaxed, everyday language and "avoids an unnecessarily formal tone", which fits Cmdr's voice.
Keep error and crash copy reassuring and factual.

## Formality

**Use the informal/direct address ("ka", "iyo"), recommended, high confidence; do NOT use the polite-plural "kayo".**
Filipino has a politeness system, but for software UI the Microsoft style guide is explicit: use "ka, iyo (do not use
formal 'kayo' or 'inyo')". So address one user with the singular informal forms; avoid the formal plural. Confidence:
high (Microsoft style guide is unambiguous here).

**The "po/opo" politeness particle: do NOT add it in UI copy, recommended.** Filipino marks deference with the particle
"po"/"ho" in speech. Software UI omits "po": it would make every button and message read as deferential spoken address,
which is wrong for a tool's voice (Microsoft's direct "ka" guidance and standard Philippine software practice both omit
it). Confidence: high; flag for David only because Cmdr's warm, personal voice is the one argument someone might make
for a touch of "po" in onboarding copy, but the default is to omit it.

**Imperatives for UI actions** (buttons, menu items): use the plain imperative verb form (the "mag-"/"-in" actor/object
focus as the action needs), consistent with the direct address above. Filipino tech UI also very commonly keeps English
verbs for actions ("Copy", "Move"); see below.

## Decision points

**Coverage is moderate, and uneven.** The pile has Microsoft terminology and the Microsoft style guide for Filipino, but
NO GNOME/Xfce catalog and NO macOS (verified 2026-06-20). So there's no open-source file-manager catalog to triangulate
file-manager terms against; the Microsoft sources are the anchor, plus a web check of how Philippine software actually
phrases things. Confidence: confirmed (about the coverage gap).

- **Script: Latin only, no decision.** Filipino uses the Latin alphabet. Confidence: high.
- **English code-switching is the central style question, and it's real.** Educated Philippine usage mixes English
  heavily ("Taglish"); much local software leaves common computing verbs and nouns in English ("File", "Folder", "Copy",
  "Settings") and translates only connective and explanatory text. Forcing pure-Tagalog coinages ("talaksan" for file,
  "pulutong" for folder) reads as stilted and is widely avoided in real UI. Recommendation: keep the common computing
  terms in English and translate the surrounding sentence into natural Filipino; this matches how Filipinos actually
  read software. Confidence: high; this is a genuine David-worth-confirming call because it sets the whole texture of
  the translation. FLAG.
- **Gender: Filipino is largely gender-neutral grammatically** (pronouns like "siya" don't mark gender; verbs don't
  agree with gender). This makes neutral phrasing easy. Confidence: high.
- **Length: Filipino runs longer than English** when fully translated (affixation and particles add length), but the
  Taglish approach keeps many terms short. Overflow-check tight buttons against the pseudolocale (`en-XA`). Confidence:
  tentative.

## Terminology and glossary

| English term | Filipino            | Notes                                                             |
| ------------ | ------------------- | ----------------------------------------------------------------- |
| Copy         | Kopyahin / Copy     | Taglish: English "Copy" common in UI; "Kopyahin" the Tagalog verb |
| Move         | Ilipat / Move       | English "Move" common in UI                                       |
| Delete       | Tanggalin / Delete  |                                                                   |
| Cancel       | Kanselahin / Cancel |                                                                   |
| file         | File                | English term standard in Philippine UI (avoid "talaksan")         |
| folder       | Folder              | English term standard (avoid "pulutong")                          |
| trash        | Basurahan           | "basura" = trash; confirm UI form                                 |

(The English/Tagalog choice per row hinges on the code-switching decision above. Defaulting to English for core
computing nouns/verbs is the recommendation.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Filipino CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('fil')`, 2026-06-20). Note: Filipino's
CLDR `one` rule is broad (covers 0 and 1 and some ranges), so write the `one` and `other` branches to read naturally
across those. Filipino often marks plural with "mga" rather than inflecting the noun, so a counted phrase may not change
the noun at all. The `desktop-i18n-plural` check enforces coverage. Confidence: confirmed (categories); tentative
(grammar phrasing).

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Never hardcode separators. The Microsoft style guide notes
  numerals are preferred over spelled-out numbers in most UI contexts.
- **Punctuation follows English conventions** (per the Microsoft style guide).
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **English code-switching (Taglish) level.** Keep core computing terms (File, Folder, Copy, Settings) in English and
  translate the surrounding sentence (recommended, matches real Philippine UI), or push for fuller Tagalog? This sets
  the texture of the whole translation. FLAG.
- **"po" politeness particle: omit (recommended) vs a light touch in personal onboarding copy.** Default omit.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/fil/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
