# Dzongkha (dz) translation style guide

Working notes for translating Cmdr into Dzongkha. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`dz`), the national language of Bhutan, written in the Tibetan script.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register where the language allows. Dzongkha has a real
software-localization tradition driven by Bhutan's Dzongkha Development Authority (the GNOME desktop and a Bhutanese
Linux distribution were localized into Dzongkha), so there is a usable precedent for plain, respectful UI tone, lean on
the GNOME catalog. Keep error and crash copy reassuring and factual.

## Formality

**Use the respectful register, recommended, with native review.** Dzongkha (like Tibetan) has honorific and
plain/ordinary registers; the honorific/respectful forms are appropriate when addressing a user. Bhutanese government
and GNOME localization use a polite register. Confidence: high that a respectful register is correct; the exact forms
need a native reviewer. Apply consistently.

**Imperatives for UI actions**: use the polite imperative consistent with the register choice, following the GNOME
Dzongkha catalog's conventions for file-manager actions.

## Decision points

**Major-product coverage is limited but not nil.** The reference pile has `dz/gnome-nautilus/` (Tier 3, verified
2026-06-20) and there is a documented Dzongkha localization effort (Dzongkha Development Authority, the
`dzongkha`-localized Linux work, Bhutanese government keyboard standards). But no commercial major ships a Dzongkha
product UI: Apple does not localize macOS into Dzongkha; Microsoft provided keyboard/script support but not a localized
Windows UI; Google's coverage is minimal. So a Dzongkha Cmdr user's OS is in English. The authoritative anchors are the
GNOME Dzongkha catalog and Dzongkha Development Authority terminology. Treat `dz` as a low-priority, native-review-
dependent locale that nonetheless has a real (if small) localization precedent to build on. Confidence: confirmed.

- **Script: Tibetan script (`dz` base).** Dzongkha is written in the Tibetan script (ཇོང་ཁ་), not Devanagari or Latin.
  This is a complex Brahmic script with stacked consonants and a syllable-delimiting tsheg mark (་). Critical: verify
  the app's text rendering shapes Tibetan correctly (stacking, vowel signs, the tsheg) and that line-breaking respects
  the tsheg, before shipping. Use a font with strong Tibetan coverage. Confidence: confirmed; rendering is the single
  biggest technical risk.
- **No grammatical gender.** Dzongkha does not grammatically gender nouns or the person addressed, so the
  gender-agreement traps of Spanish/Greek/Dogri do not apply. The register (honorific vs plain) is the live distinction,
  not gender. Confidence: high.
- **Regional variant: none.** Dzongkha is specific to Bhutan; there is no region split. Confidence: confirmed.
- **Length and rendering, not raw character count, are the layout concern.** Tibetan glyphs and stacking can need more
  vertical space and careful baseline handling; overflow- and clipping-check against real rendered text, not just the
  pseudolocale. Confidence: high.

## Terminology and glossary

Dzongkha file-manager terms should come from the GNOME Dzongkha catalog and Dzongkha Development Authority terminology,
confirmed by native review. Leave this table to be populated from those sources rather than guessing.

- Glossary: populate from GNOME dz catalog + DDA terminology, via native review (populate via the cited sources and
  native review; nothing guessed yet).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. Brand
words in Latin script sit inside Tibetan-script runs; verify they render and don't break bidi/line-breaking oddly.

## Plurals

Dzongkha CLDR categories: `other` only, a single category (verified with `new Intl.PluralRules('dz')`, 2026-06-20; GNOME
nautilus uses `nplurals=2; plural=(n!=1)` but CLDR treats Dzongkha as having no count-based plural distinction). This is
the simplest plural language in Cmdr's set: one `other` branch covers every count. Don't invent a singular/plural split
the language doesn't make; phrase counted strings to read correctly for any number with a single form. Confidence:
confirmed.

## Notes and decisions

- **Tsheg and punctuation**: Dzongkha uses the Tibetan tsheg (་) between syllables and the shad (།) as a clause/sentence
  terminator rather than the Latin period. A native reviewer handles this; don't impose Latin punctuation.
- **Digits**: Dzongkha may use Tibetan digits (༠༡༢…) or Western digits; let the formatter layer decide, don't hardcode.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Dzongkha is a small-audience locale with no commercial precedent**, though it has a real government/GNOME
  localization tradition. Confirm whether it's worth attempting this round.
- **Tibetan-script rendering** must be verified in the actual app (stacking, tsheg, line-breaking) before any Dzongkha
  ship, this is a code/rendering check, not just a translation one. Flag for David as a prerequisite.
- **Register forms** (honorific vs plain) need a native Dzongkha reviewer to pin down.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/dz/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
