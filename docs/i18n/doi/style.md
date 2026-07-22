# Dogri (doi) translation style guide

Working notes for translating Cmdr into Dogri. Read `../README.md` for how this fits the translation process.

This is the language base (`doi`). The reference pile keys it `doi-Deva` (Devanagari script); see the script decision
point below for why the base `doi` tag is the right one for Cmdr.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. There is little to no
precedent for Dogri software UI tone (see Decision points), so lean on the writing-style guide's principles and on how
related Indo-Aryan languages (Hindi, especially) phrase friendly software copy. Keep error and crash copy reassuring and
factual.

## Formality

**Use a polite register, recommended, but with low confidence, flag for David.** Dogri, like Hindi and other Indo-Aryan
languages, has a T-V-style distinction in second-person address (an informal vs a polite/respectful form). For software
addressing an unknown adult, the polite form is the safe default, consistent with how Hindi UI addresses users
respectfully ("आप"-style register). Confidence: tentative, there is essentially no Dogri software corpus to anchor
against, so this is a reasoned default, not an observed one. A native Dogri reviewer should confirm the exact forms;
whatever is chosen, apply it consistently.

**Imperatives for UI actions**: use the polite imperative consistent with the formality choice. No reference catalog of
Dogri button labels exists, so terms will need native review.

## Decision points

**Major-product coverage is essentially nil, this is the headline finding, a strong low-priority signal.** The reference
pile has ONLY `doi-Deva/microsoft-style-guides/` (a single Microsoft style-guide PDF) and no macOS, no terminology
database, no GNOME/Xfce catalog (verified 2026-06-20). Dogri is a scheduled language of India (~2.5 million speakers,
chiefly the Jammu region) but has almost no localized consumer software: Apple ships no Dogri; Microsoft's support is
limited to the style guide and a little terminology; Google's coverage is thin. A Dogri Cmdr user's OS is in English or
Hindi. Treat `doi` as a very-low-priority, native-review-dependent locale. Confidence: confirmed.

- **Script: Devanagari (`doi` base = Devanagari).** Dogri was historically written in the Dogra/Takri script, but modern
  Dogri is written in Devanagari (देवनागरी), which the Indian government and all current usage adopt. There is no live
  competing modern script to choose between, so the base `doi` tag implies Devanagari and no `doi-Deva` vs `doi-Takri`
  split is needed for Cmdr. Render with a font that covers Devanagari well. Confidence: confirmed.
- **Gender: Dogri has grammatical gender (masculine/feminine) that affects verb and adjective agreement**, including
  agreement with the person addressed in some constructions. This is a real translation trap: a verb agreeing with the
  user may need the user's gender, which the UI doesn't know. Prefer phrasings that avoid gender agreement with the user
  (impersonal or neutral constructions). A native reviewer is essential here. Confidence: high that the issue exists;
  the handling needs native review.
- **Regional variant: none meaningful.** Confidence: confirmed.
- **Rendering: complex-script shaping.** Devanagari needs proper conjunct (ligature) shaping and matra positioning;
  verify the app's text rendering handles it (it should, via the system text stack) and overflow-check. Confidence:
  high.

## Terminology and glossary

No reliable Dogri file-manager term source exists in the pile. Terms must come from native review (or, as a starting
point for the reviewer, the corresponding Hindi term, since Dogri shares much vocabulary with Hindi). Leave this table
to be filled by a native reviewer.

- Glossary: populate via native review; Hindi is a reasonable starting reference (populate via the cited sources and
  native review; nothing guessed yet).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Dogri CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('doi')`, 2026-06-20). Write both branches.
Indo-Aryan plural and counted-noun agreement interacts with gender and case; get the noun form right in each branch.
Native review needed for the exact forms.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Dogri may use Devanagari or Western digits depending on
  convention; let the formatter decide, don't hardcode.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Priority: Dogri has almost no software-localization precedent and a small audience.** It's a very-low-priority
  locale that depends entirely on native review to do well. Confirm whether it's worth attempting at all this round.
- **Formality forms** (tentative): polite register recommended, but the exact second-person forms need a native Dogri
  reviewer, not agent guesswork.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/doi/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
