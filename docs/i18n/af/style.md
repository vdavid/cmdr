# Afrikaans (af) translation style guide

Working notes for translating Cmdr into this language. Read [`README.md`](../README.md) for how this fits the
translation process.

## Voice and tone

Friendly, concise, active, calm. Afrikaans UI copy in major products reads plain and direct; match Cmdr's English
register without stiffness. Error messages stay calm and actionable, avoiding alarm.

## Formality

Use the informal second person `jy` / `jou` for the user, not formal `u`. This matches modern Afrikaans software
practice (Microsoft, GNOME) and Cmdr's friendly voice. `u` reads as letter/officialese register and would feel cold for
a personal file manager. Imperatives for buttons and menu items use the plain verb stem (for example "Open", "Kopieer",
"Vee uit"). Confidence: high.

## Terminology and glossary

| English term | This language         | Notes                                                                       |
| ------------ | --------------------- | --------------------------------------------------------------------------- |
| file         | lêer                  |                                                                             |
| folder       | gids / omslag / vouer | GNOME uses "gids", Microsoft uses "omslag"; pick one and keep it consistent |
| copy         | kopieer               |                                                                             |
| move         | skuif                 |                                                                             |
| delete       | vee uit               |                                                                             |
| trash        | asblik                |                                                                             |
| search       | soek                  |                                                                             |

(Confirm "gids" vs "vouer" for folder against the Microsoft Afrikaans terminology in the reference pile; record the
final call here.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Run `new Intl.PluralRules('af').resolvedOptions().pluralCategories` (Afrikaans is `one` / `other`, like English). Two
branches per plural message.

## Decision points

- **Formality: informal `jy`, not `u` (the main call).** Settled above. Afrikaans cleanly distinguishes `jy` (familiar)
  from `u` (formal). A consumer app aimed at individuals uses `jy`; reserve `u` for nothing here. This is the single
  decision that shapes the whole catalog's tone. Confidence: high.
- **Latin script, no script variants.** Afrikaans is Latin-script, LTR, with diacritics (ê, ô, ë, ï, é). No
  Cyrillic/other variant, no RTL. Just ensure the font and input handle the diacritics. Confidence: high.
- **Regional variant: none needed.** Afrikaans is effectively centered on South Africa (`af-ZA`); the base tag `af`
  covers it. There's no second standardized region to fork for. Confidence: high.
- **Reference availability.** Good coverage: the pile has Microsoft style guide + terminology and GNOME Nautilus, so
  terms and tone have solid precedent to lean on. Confidence: high.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/af/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
