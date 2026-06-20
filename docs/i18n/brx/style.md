# Bodo (brx) translation style guide

Working notes for translating Cmdr into Bodo (बड़ो / बर'). Read [`README.md`](../README.md) for how this fits the
translation process.

`brx` is the language base, written in the Devanagari script. Bodo is a Sino-Tibetan language, one of India's scheduled
languages, spoken mainly in Assam. The reference pile has only the Microsoft style guide for `brx` (Tier 2); no macOS,
no GNOME, no Microsoft terminology TBX. Low-resource, but Microsoft's style guide gives clear formality and script
guidance.

## Voice and tone

Friendly, concise, active, calm, never alarmist. The Microsoft Bodo style guide steers toward "formal, standard,
appropriate but friendly", avoiding "harsh, dialect and unprofessional language" (verified in
`brx/microsoft-style-guides/`, 2026-06-20). That "formal but friendly" register fits Cmdr's voice well. Keep error and
crash copy reassuring and factual; never use the bare labels "error"/"failed".

## Formality

**Address the user with the honorific "you", settled from Microsoft.** The Microsoft Bodo style guide is explicit:
"Always address pronoun 'you' in honorific way", नोंथां for singular, नोंथांमोन for plural (verified in
`brx/microsoft-style-guides/`, 2026-06-20). Use the honorific singular नोंथां when addressing the user. Confidence: high
(authoritative single source). A native reviewer confirms the verb forms.

**Imperatives for UI actions**: use the honorific-consistent imperative for buttons and menu items, following the
Microsoft Bodo style guide's grammar conventions.

## Decision points

The defining facts are script (Devanagari) and the honorific register, both settled by Microsoft. There's no script or
variant call to make.

- **Script: Devanagari (`brx` base), settled.** Bodo is officially written in Devanagari (since 1963; it has been
  written in Latin and Assamese scripts historically, but Devanagari is the standard). Microsoft localizes Bodo in
  Devanagari. Recommendation: Devanagari. Confidence: confirmed.
- **Capitalization does not apply.** The Microsoft Bodo style guide states: "Capitalization does not apply to Bodo
  (Devanagari). A single form is used for each letter" (verified 2026-06-20). So Cmdr's English sentence-case rule has
  no Bodo analogue; don't try to impose casing. Confidence: confirmed.
- **No grammatical gender trap for the address.** Bodo (Sino-Tibetan) does not gender the address pronoun the way
  Romance/Slavic languages do; the honorific register is the live distinction. Confidence: high.
- **Regional variant: none worth splitting.** Bodo is centered on Assam with a single literary standard; no
  product-level region split. Confidence: high.
- **Devanagari rendering for a Sino-Tibetan language.** Bodo uses Devanagari but with its own conventions; verify the
  app renders Devanagari conjuncts and the Bodo-specific usage correctly with a font that has full Devanagari coverage.
  Confidence: high; a rendering check.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. The only source for `brx` is the Microsoft style guide
(prose/tone, not a term database), so there's no mined term list. File-manager terms must come from native review and
any Bodo computing glossary; mark everything `tentative` until reviewed.

- Glossary: no term source in the pile (style guide only); populate via native review (populate via the cited sources
  and native review; nothing guessed yet).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. Latin
brand words sit inside Devanagari runs; verify they render cleanly.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('brx')`, 2026-06-20). Two branches. The Microsoft
guide notes distinct honorific singular (नोंथां) and plural (नोंथांमोन) address forms, so mind that plural agreement
shows up in the pronoun/verb; write both count branches with correct agreement. A native reviewer confirms.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Bodo (Devanagari) may use Devanagari digits (०१२…) or Western
  digits; let the formatter decide, don't hardcode.
- **No capitalization** (see Decision points): don't apply English casing rules.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: low-resource locale with only a Microsoft style guide (no term database) in the pile.** Confirm whether
  it's worth attempting this round. Formality (honorific नोंथां) and script (Devanagari, no capitalization) are settled;
  every term needs native review since the pile has no Bodo term source.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/brx/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
