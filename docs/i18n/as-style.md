# Assamese (as) translation style guide

Working notes for translating Cmdr into Assamese. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Assamese.

**Sparse pile, no macOS.** Apple ships no Assamese macOS UI. The pile has GNOME Nautilus for `as`, plus an `as-IN`
folder with a Microsoft terminology glossary and a Microsoft style guide (`_ignored/i18n/as/` and `as-IN/`). No Xfce, no
macOS. Assamese is the official language of Assam, India (~15M speakers), written in the Assamese (Eastern Nagari)
script. Terms lean on GNOME + Microsoft. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone.

- **Address form: honorific/polite imperative - recommended (high).** Assamese (like other Indic languages) has
  honorific verb forms; software uses the polite imperative (the `-ক`/`কৰক` form). GNOME Assamese shows polite
  imperatives ("বাতিল কৰক" = cancel, "সন্ধান" = search) (verified 2026-06-20). Recommended default: **polite imperative
  throughout.** Flag because it sets the tone for every action label.
- **Script and font readiness (high).** The Assamese script is the Bengali-Assamese (Eastern Nagari) script with two
  Assamese-specific letters (ৰ ra, ৱ wa) that distinguish it from Bengali. Unicode shares the Bengali block
  (U+0980-U+09FF). Font caveat: the app's font stack must render the Bengali/Assamese block AND the Assamese-specific
  letters; verify (app-readiness check, like font coverage for Amharic). Flag for David.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. MS Assamese follows the general
Microsoft voice ("warm and relaxed, less formal", verified 2026-06-20), with attention to gender-neutral phrasing (the
style guide has an "avoid gender bias" section). With no macOS reference, prioritize clear, plain Assamese. Error
messages stay calm and actionable: phrase the problem and the next step.

## Formality

Assamese verbs distinguish honorific levels (familiar vs honorific 2nd person). Software uses the honorific/polite form.

- **Polite imperative, throughout.** The polite imperative is formed with `-ক`/`কৰক` ("কৰক" = please do). GNOME Assamese:
  "বাতিল কৰক" (Cancel), and the MS guide uses the honorific register (verified 2026-06-20).
- **Action labels (buttons, menu items): polite imperative.** "সন্ধান" (Search), "বাতিল কৰক" (Cancel). So the rule:
  **labels and user-facing instructions both use the polite imperative; never the familiar/informal form.** Confidence:
  high (GNOME and MS agree on the honorific register).

## Decision points

- **Script: Assamese (Eastern Nagari).** Covered as the headline flag above, including the font-readiness gate and the
  Assamese-specific ৰ/ৱ letters (do NOT substitute the Bengali র/ব forms - a native reader sees that as Bengali, not
  Assamese). Confidence: high.
- **Regional variant: one, `as` (`as-IN`).** Assamese is official in Assam, India; no second national standard, no
  variant matrix. Confidence: high.
- **Gender / inclusive language (a genuine concern, tentative on the fix).** Assamese verbs are largely gender-neutral
  (unlike Hindi, Assamese does not mark verb gender agreement with the subject), which is a relief - the
  user-gender-agreement problem that plagues Hindi/Marathi mostly doesn't arise. But nouns referring to people can be
  gendered; the MS guide's "avoid gender bias" section applies. Recommendation: use the verb's gender-neutrality, and
  rephrase any person-referring noun neutrally. Confidence: tentative (verify with a native reviewer).
- **Numerals: Western (0-9) vs Assamese numerals (high).** Assamese has its own digit forms (০১২৩…, shared with the
  Bengali block), but Western Arabic digits dominate in modern software. Recommendation: Western digits via `Intl`.
  Confidence: high.
- **Capitalization: not applicable.** The Assamese script has no case distinction; the sentence-case rule is moot for
  Assamese text (still applies to embedded Latin brand tokens). Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/as/` (GNOME Nautilus) and
`as-IN/` (MS terminology, MS style guide; NO macOS) on 2026-06-20. Sources decide the term; Cmdr writes its own value
(MS copyrighted, GNOME GPL, never copied verbatim). Without macOS, terms are `tentative` unless GNOME and MS clearly
agree.

- **folder: `ফোল্ডাৰ`** · GNOME ("ফোল্ডাৰ", a transliteration of "folder" with the Assamese ৰ). `high`.
- **trash: `আৰ্বজনা` (refuse/garbage)** · GNOME ("আৰ্বজনা"). Verify against MS; "আবৰ্জনা" is the more standard spelling.
  `tentative`.
- **cancel: `বাতিল কৰক`** · GNOME ("বাতিল কৰক"). Polite imperative. `high`.
- **search: `সন্ধান`** · GNOME ("সন্ধান"). `high`.
- **file, open, rename, eject, volume, pane, tab, bookmark** · no clean GNOME match captured; defer to a native reviewer
  using the MS `as-IN` glossary. `tentative`.

Add terms as they come up; triangulate GNOME + the MS `as-IN` glossary, and record confidence. Native review is
mandatory given the thin sources.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand tokens
sit inside Assamese-script text; keep them Latin. Common terms like "folder" are often transliterated rather than
translated in Assamese UI - that's a translation choice for the glossary, distinct from the do-not-translate brand list.

## Plurals

CLDR categories for `as`: `one`, `other` (verified with `new Intl.PluralRules('as')`). Only two forms. Note: Assamese
`one` groups 0 and 1 (CLDR Indic pattern), so don't assume 0 falls into `other`. The `desktop-i18n-plural` check
requires both categories.

## Notes and decisions

- **Punctuation.** Assamese traditionally uses the danda `।` (U+0964) as a full stop for sentences, though modern UI
  often uses the Western period. Defer the exact convention to a native reviewer.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` produce locale-correct output
  (Western digits for `as`). The Indian digit-grouping (lakh/crore, 1,00,000) is an `Intl` concern, not a string
  concern; never hardcode grouping. Verify the grouping with a reviewer.
- **Length.** Indic conjuncts are compact per glyph but tall; length is a moderate risk - overflow-check against the
  pseudolocale (`en-XA`), and watch line height for stacked conjuncts/matras.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
