# Gujarati (gu) translation style guide

Working notes for translating Cmdr into Gujarati (ગુજરાતી). Read `../README.md` for how this fits the translation
process.

This is the language base (`gu`), the universal Gujarati set. Gujarati is written in the Gujarati script (an abugida,
Brahmic family) and is the language of Gujarat, India; a single standard, no region variant needed.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Microsoft's Gujarati
style guide targets warm, conversational, "not unnecessarily formal" copy that politely addresses the user. Keep error
and crash copy reassuring and factual.

## Formality

**Address the user with the polite "તમે" (tame), recommended, high confidence.** Gujarati has a politeness distinction:
familiar "તું" (tu) vs polite "તમે" (tame). For software addressing an unknown adult, the polite "તમે" is the Indic-UI
standard (parallel to Hindi "आप"): respectful without being stiff. Microsoft's Gujarati style guide uses the polite
second person to "politely ask the user". "તું" would read as over-familiar. Use polite verb forms consistently.
Confidence: high (Microsoft style guide; consistent with the wider Indic-UI norm).

**Imperatives for UI actions** (buttons, menu items): use the polite imperative, matching the GNOME catalog: "નકલ કરો"
(copy), "કાઢી નાખો" (delete), "રદ કરો" (cancel). The "-o" ending (કરો) is the polite-imperative form that agrees with
તમે.

## Decision points

**Coverage is good.** GNOME, Microsoft terminology, and the Microsoft style guide all exist for Gujarati (verified
2026-06-20); macOS is missing (Apple does not localize macOS into Gujarati, so the user's Finder chrome is in English).
Confidence: confirmed.

- **Script: Gujarati script only, no decision.** Gujarati is written in its own Brahmic abugida. Never romanize UI
  strings. Confidence: high.
- **Conjunct consonants and matras (vowel signs) need correct Unicode composition.** Gujarati stacks consonants into
  conjuncts (with the virama / halant) and attaches vowel signs around the base. Translate in properly composed Unicode;
  don't break a conjunct or strand a matra. Editors and fonts that don't shape Gujarati can silently mangle this.
  Confidence: high.
- **Honorific verb agreement is the formality mechanic.** With "તમે", verbs and imperatives take the polite plural-form
  ending; mixing familiar and polite forms in one flow reads inconsistently. Pick polite and apply it everywhere.
  Confidence: high.
- **English loanwords are common and expected in tech UI.** Gujarati tech copy freely uses transliterated English terms
  ("ફાઇલ" = file, "ફોલ્ડર" = folder) rather than forcing Sanskrit-derived coinages. Follow the Microsoft/GNOME choice
  per term; don't over-Sanskritize. Confidence: high (Microsoft + GNOME agree).
- **Gender: Gujarati has grammatical gender** (verbs and adjectives agree with the subject's gender), but addressing the
  user with polite "તમે" + the polite verb ending avoids gendering them in most UI sentences. Keep phrasing that doesn't
  force a gendered past-tense or adjective on the user. Confidence: high.
- **Length: Gujarati is usually compact** (comparable to or slightly shorter than English in width, though tall stacked
  conjuncts need vertical room). Overflow risk is lower than the Romance/Celtic languages, but check line height in
  tight rows against the pseudolocale. Confidence: tentative.

## Terminology and glossary

| English term | Gujarati  | Notes                                      |
| ------------ | --------- | ------------------------------------------ |
| Copy         | નકલ કરો   | GNOME ("નકલ કરો (\_C)")                    |
| Move         | ખસેડવું   | Microsoft                                  |
| Delete       | કાઢી નાખો | GNOME ("કાઢી નાખો (\_D)")                  |
| Cancel       | રદ કરો    | GNOME ("રદ કરો (\_C)")                     |
| file         | ફાઇલ      | Microsoft + GNOME (transliterated English) |
| folder       | ફોલ્ડર    | Microsoft + GNOME (transliterated English) |
| trash        | કચરાપેટી  | GNOME (lit. "rubbish bin")                 |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Gujarati CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('gu')`, 2026-06-20; GNOME nautilus uses
`nplurals=2; plural=(n!=1)`). Note: in Gujarati's CLDR rule, fractional and zero values fall into `other`, and `one`
covers n=1 (and 0..1 ranges for some uses), so write the `other` branch to read naturally for 0 and large counts. The
`desktop-i18n-plural` check enforces coverage. Confidence: confirmed.

## Notes and decisions

- **Numbers**: Gujarati can use Gujarati-script digits (૦-૯) or Western digits; modern UI typically follows the locale
  formatter. Never hardcode digit shapes or separators; let the formatter layer decide.
- **Dates come from the formatter layer.** Never hardcode.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- None outstanding. Polite "તમે" address is well-anchored by Microsoft and the Indic-UI norm.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/gu/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
