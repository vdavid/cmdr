# Galician (gl) translation style guide

Working notes for translating Cmdr into Galician. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`gl`), the universal Galician set. Galician is co-official in Galicia (Spain) and has a
single standard written norm (the Real Academia Galega / ILG norm); no region variant is needed.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Microsoft's Galician
style guide explicitly aims for warm, relaxed, everyday language and "avoids an unnecessarily formal tone", which fits
Cmdr's voice well. Keep error and crash copy reassuring and factual.

## Formality

**Use the informal "ti" (second person singular), recommended, high confidence.** Galician has a T-V distinction (ti vs
vostede), but the dominant software convention is informal direct address. Microsoft's Galician style guide is explicit:
"Use an informal tone as a general rule. Use the second person to address the user, but omit the subject pronoun ti
whenever possible" (the verb already implies the person). GNOME Galician is likewise informal. So: address the user with
ti-conjugated verbs, drop the explicit pronoun where the verb makes it clear. This matches both Cmdr's warm voice and
the Galician-software norm, so there's nothing for David to settle here.

**Imperatives for UI actions** (buttons, menu items): use the infinitive, matching the Galician software convention
("Copiar", "Eliminar", "Cancelar"). This is the Spanish/Galician UI pattern: menu and button labels are infinitives, not
imperatives.

## Decision points

**Coverage is good for this language.** The reference pile carries GNOME, Xfce, Microsoft terminology, and the Microsoft
style guide for Galician (verified 2026-06-20); only macOS is missing (Apple does not localize macOS into Galician, so a
Galician Cmdr user's Finder chrome is in Spanish or English). The Microsoft style guide is the strongest formality
anchor here. Confidence: confirmed.

- **Script: Latin only, no decision.** Galician is written in the Latin alphabet. The one live orthographic question
  (the RAG/ILG "reintegrationist" debate over Portuguese-style spelling) does not affect UI translation: use the
  official RAG norm, which every major (Microsoft, GNOME) follows. Confidence: high.
- **Don't translate from Spanish or Portuguese by reflex.** Galician sits between the two and shares much vocabulary,
  but has its own terms: "ficheiro" (file, not Spanish "archivo"), "cartafol" (folder, not "carpeta"), "lixo" (trash).
  Translate from English using the Galician sources, not by adapting a Spanish string. Confidence: high (Microsoft +
  GNOME agree).
- **Gender: Galician has grammatical gender** (masculine/feminine on nouns and adjectives), but informal direct address
  with verbs avoids gendering the user. No inclusive-ending debate (like Spanish's -e/-x) needs to surface in a file
  manager: keep direct, verb-based address and neutral phrasing. Confidence: high.
- **Length: Galician runs longer than English** (Romance expansion, similar to Spanish, roughly 15–25%). Overflow-check
  tight buttons against the pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

| English term | Galician | Notes                                     |
| ------------ | -------- | ----------------------------------------- |
| Copy         | Copiar   | Microsoft + GNOME                         |
| Move         | Mover    | GNOME                                     |
| Delete       | Eliminar | GNOME ("\_Eliminar")                      |
| Cancel       | Cancelar | GNOME ("\_Cancelar")                      |
| file         | ficheiro | Microsoft + GNOME (NOT Spanish "archivo") |
| folder       | cartafol | Microsoft + GNOME (NOT Spanish "carpeta") |
| trash        | Lixo     | GNOME                                     |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Galician CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('gl')`, 2026-06-20; GNOME nautilus uses
`nplurals=2; plural=(n != 1)`). Simple two-form system like English. The `desktop-i18n-plural` check enforces coverage.
Confidence: confirmed.

## Notes and decisions

- **Diacritics**: Galician uses the acute accent (á é í ó ú) and the diaeresis (ü), plus ñ. These are meaningful; keep
  them.
- **Numbers and dates come from the formatter layer.** Never hardcode separators (Galician uses a comma decimal
  separator and a period or space for thousands).
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- None outstanding. Formality (informal ti) is well-anchored by the Microsoft style guide and GNOME; flag only if David
  wants to override the norm.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/gl/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
