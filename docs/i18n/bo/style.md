# Tibetan (bo) translation style guide

Working notes for translating Cmdr into Tibetan (བོད་སྐད་). Read [`README.md`](../README.md) for how this fits the
translation process.

`bo` is the language base, written in the Tibetan script. The reference pile has only GNOME nautilus for `bo` (Tier 3);
no macOS, no Microsoft localized UI. This is a low-resource locale with a small but real localization tradition.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register where the language allows. Keep error and
crash copy reassuring and factual. There is a Tibetan free-software localization tradition (GNOME, Tibetan computing
projects), so there is some precedent for plain, respectful UI tone; lean on the GNOME Tibetan catalog.

## Formality

**Use the honorific/respectful register (ཞེ་ས་, "shesa"), recommended, with native review.** Tibetan has honorific and
plain/ordinary registers; the honorific (shesa) signifies respect and politeness and is appropriate when addressing a
user (verified via web research, 2026-06-20). Tibetan localization efforts use a respectful register. Confidence: high
that respectful is correct; the exact honorific forms need a native reviewer. Apply consistently.

**Imperatives for UI actions**: use the polite imperative consistent with the register, following the GNOME Tibetan
catalog's conventions for file-manager actions.

## Decision points

The defining concern is script rendering, not formality or variant. Major commercial UI coverage is near-nil.

- **Major-product coverage is minimal.** No major ships a Tibetan product UI: Apple provides a Tibetan font and input on
  iOS/macOS but does not localize macOS into Tibetan; Microsoft added Tibetan to its translation service (2021) and
  provides script/input support but no localized Windows UI; Google's coverage is partial. So a Tibetan Cmdr user's OS
  is in English or Chinese. The authoritative anchors are the GNOME Tibetan catalog and the Tibetan free-software /
  computing community's terminology. Treat `bo` as a low-priority, native-review-dependent locale with a small
  precedent. Confidence: high.
- **Script: Tibetan script (`bo` base), and rendering is the biggest technical risk.** Tibetan is a complex Brahmic
  script with stacked consonants (head/sub-joined letters), vowel signs, and a syllable-delimiting tsheg mark (་).
  Critical: verify the app shapes Tibetan correctly (stacking, vowel placement, the tsheg) and that line-breaking
  respects the tsheg, with a font that has strong Tibetan coverage, BEFORE shipping. This is a code/rendering check, not
  just a translation one. Confidence: confirmed; rendering is the single biggest risk.
- **No grammatical gender.** Tibetan does not grammatically gender nouns or the person addressed, so the
  gender-agreement traps of Romance/Slavic languages don't apply. The register (honorific vs plain) is the live
  distinction, not gender. Confidence: high.
- **Regional variant: none worth splitting.** "Tibetan" here is standard/literary Tibetan (Central Tibetan basis). Amdo
  and Kham are spoken variants but the written standard is shared; no product-level split. (Dzongkha is a separate
  language, `dz`, not a Tibetan variant.) Confidence: high.

## Terminology and glossary

Tibetan file-manager terms should come from the GNOME Tibetan catalog and the Tibetan computing community's terminology,
confirmed by native review. Leave the table to be populated from those sources rather than guessing.

- Glossary: populate from `bo/gnome-nautilus/nautilus.po` + Tibetan-computing terminology, via native review (populate
  via the cited sources and native review; nothing guessed yet).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. Latin
brand words sit inside Tibetan-script runs; verify they render and don't break line-breaking or the tsheg-based layout
oddly.

## Plurals

CLDR categories: `other` only (verified with `new Intl.PluralRules('bo')`, 2026-06-20). A single category, like
Dzongkha: one `other` branch covers every count. Don't invent a singular/plural split the language doesn't make; phrase
counted strings to read correctly for any number with a single form. Confidence: confirmed.

## Notes and decisions

- **Tsheg and punctuation**: Tibetan uses the tsheg (་) between syllables and the shad (།) as a clause/sentence
  terminator rather than the Latin period. A native reviewer handles this; don't impose Latin punctuation.
- **Digits**: Tibetan may use Tibetan digits (༠༡༢…) or Western digits; let the formatter layer decide, don't hardcode.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: small-audience locale with no commercial precedent**, though it has a real GNOME/community localization
  tradition. Confirm whether it's worth attempting this round.
- **Tibetan-script rendering** must be verified in the actual app (stacking, tsheg, line-breaking) before any Tibetan
  ship. This is a code/rendering prerequisite, not just a translation one. Flag for David.
- **Honorific (shesa) register forms** need a native Tibetan reviewer to pin down.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/bo/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
