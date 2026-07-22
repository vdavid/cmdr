# Friulian (fur) translation style guide

Working notes for translating Cmdr into Friulian (furlan). Read `../README.md` for how this fits the translation
process.

This is the language base (`fur`), the universal Friulian set. Friulian is a Rhaeto-Romance language of the Friuli
region (northeastern Italy), with an official standard orthography (the OLF / ARLeF koiné). No region variant needed for
UI purposes.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Friulian has an
unusually active GNOME localization community, so the GNOME catalog gives a real, consistent register to lean on. Keep
error and crash copy reassuring and factual.

## Formality

**Use informal direct address ("tu", singular), recommended, high confidence.** Friulian has a T/V distinction (tu vs
the polite "vô"), but software convention follows Italian open-source practice, which is informal direct address for the
user. The GNOME Friulian catalog is informal. Address one user with "tu". Confidence: high (GNOME Friulian + Italian-UI
norm).

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME catalog: "Copie" (copy),
"Elimine" (delete), "Scancele" (cancel).

## Decision points

**This is a thin-coverage locale, but the GNOME anchor is strong.** The reference pile has ONLY a GNOME nautilus catalog
(Tier 3) for Friulian, well-translated (~85%, verified 2026-06-20): no macOS, no Microsoft terminology, no Microsoft
style guide. Apple and Microsoft do not localize into Friulian, so the user's OS chrome is in Italian or English. But
Friulian's GNOME community localization is mature and consistent, so the file-manager terms are well-anchored despite
the single source. Confidence: confirmed (about the coverage).

- **Script: Latin only, no decision.** Friulian uses the Latin alphabet with the official ARLeF orthography (including
  ç, and the circumflex on long vowels: â ê î ô û). Keep the diacritics; they're part of the standard spelling.
  Confidence: high.
- **Use the official ARLeF/OLF koiné, not a local variety.** Friulian has dialect variation (Central, Western, Carnian),
  but the standard written koiné is what GNOME and official Friulian materials use. Translate in the koiné. Confidence:
  high.
- **Don't translate from Italian by reflex.** Friulian is a distinct Romance language, not an Italian dialect: "file" =
  "file" (English loan, as in Italian) but folder = "cartele", delete = "elimine". Use the Friulian GNOME terms.
  Confidence: high (GNOME).
- **Gender: Friulian has grammatical gender** (masculine/feminine), but direct tu-address doesn't gender the user.
  Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: Friulian runs longer than English** (Romance expansion, ~15–25%). Overflow-check tight buttons against the
  pseudolocale (`en-XA`). Confidence: tentative.

## Terminology and glossary

| English term | Friulian  | Notes                                               |
| ------------ | --------- | --------------------------------------------------- |
| Copy         | Copie     | GNOME ("\_Copie")                                   |
| Delete       | Elimine   | GNOME ("\_Elimine")                                 |
| Cancel       | Scancele  | GNOME ("S_cancele")                                 |
| folder       | Cartele   | GNOME                                               |
| file         | file      | GNOME (English loan, as in Italian)                 |
| trash        | Scovacere | GNOME                                               |
| Move         | (confirm) | not in the sampled GNOME entries; check the catalog |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Friulian CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('fur')`, 2026-06-20; GNOME nautilus uses
`nplurals=2; plural=(n != 1)`). Simple two-form system like English. The `desktop-i18n-plural` check enforces coverage.
Confidence: confirmed.

## Notes and decisions

- **Diacritics**: keep ç and the circumflex vowels (â ê î ô û); they're part of the official orthography.
- **Apostrophe**: Friulian uses apostrophes in elisions ("l'ute", "un'ore"), so ICU apostrophe-doubling matters here.
- **Numbers and dates come from the formatter layer.** Never hardcode separators (Friulian follows Italian: comma
  decimal).
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Priority: Friulian is a thin-coverage locale** (GNOME only, no macOS/Microsoft), though the GNOME anchor is mature.
  Confirm it's worth doing.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/fur/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
