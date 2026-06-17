# <Language name> (<tag>) translation style guide

Working notes for translating Cmdr into this language. Copy this template to `<tag>-style.md` and fill every section
before the first translation pass. Read [`README.md`](README.md) for how this fits the translation process.

## Voice and tone

How Cmdr should sound in this language. Cmdr's English voice is friendly, concise, active, and never alarmist (error
messages stay calm and actionable, and avoid the words "error" and "failed"). State the equivalent register for this
language.

## Formality

The form of address to use (formal vs informal second person, where the language distinguishes them), and any
conventions for imperatives in UI actions (buttons, menu items).

## Terminology and glossary

The agreed translation for recurring product and file-manager terms (for example: pane, tab, volume, listing, transfer,
trash, viewer). One row per term so the whole catalog stays consistent. Add terms as they come up.

| English term | This language | Notes |
| ------------ | ------------- | ----- |

## Brand and do-not-translate

Keep these verbatim (they are product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri,
Rust, Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the
`desktop-i18n-dont-translate` check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

This language's CLDR plural categories (run `new Intl.PluralRules('<tag>').resolvedOptions().pluralCategories`), and any
grammar notes a translator needs (gender or case agreement that interacts with counts). The `desktop-i18n-plural` check
requires every plural message to cover the categories this language needs.

## Notes and decisions

Anything else: punctuation conventions, quotation marks, number and date phrasing peculiar to this language, and any
case-by-case rulings made during translation so they are not relitigated.
