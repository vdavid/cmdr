# Amharic (am) translation style guide

Working notes for translating Cmdr into Amharic. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Amharic.

**Sparse pile, no macOS.** Apple ships no Amharic macOS UI. The pile has GNOME Nautilus + Xfce Thunar for `am`, plus an
`am-ET` folder with a Microsoft terminology glossary and a Microsoft style guide (`_ignored/i18n/am/` and `am-ET/`). No
macOS Finder. Amharic is the working language of Ethiopia (~30M+ speakers). Terms lean on GNOME + Microsoft. Evidence
verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. The first two carry real weight.

- **Script: Ge'ez / Ethiopic (`Ethi`) - no real alternative, but a font-readiness flag (high).** Amharic is written in
  the Ge'ez (Ethiopic) abugida, Unicode block U+1200-U+137F. There is no competing script. Recommendation: target
  Ethiopic. Font caveat: the app's font stack MUST cover the Ethiopic block; the macOS system font does not render it by
  default, so this is an app-readiness check (parallel to how RTL is a readiness gate for Hebrew). Flag for David.
  Confidence: high.
- **Address form: informal singular imperative - recommended, and unusually so (high).** Unlike most languages where
  software uses the polite form, the Microsoft Amharic guide explicitly chooses the INFORMAL tone: "the informal tone
  has been used in the localization in Amharic… No sense of disrespect can be inferred", and the formal (plural-
  conjugated) verb "would be contrary to the spirit of ICT and would impose an unnecessary burden on communication" (MS
  style guide, verified 2026-06-20). So for Amharic, informal singular is the CORRECT software register, not a warmth
  liberty. Recommended default: **informal singular.** Flag so it's a conscious, documented choice.

## Voice and tone

Friendly, concise, active, calm, never alarmist. The informal register (above) actually aligns Amharic naturally with
Cmdr's warm-informal English voice. MS Amharic adds: **avoid the passive voice** - it "suggests a stubborn refusal to be
polite"; politeness is carried by tone variation, not by passivization (verified 2026-06-20). So phrase actively and
directly. With no macOS reference, prioritize clear, plain Amharic. Error messages stay calm and actionable: phrase the
problem and the next step.

## Formality

- **Informal singular, throughout.** This is the documented Amharic software norm (see the flag above), not a casual
  choice. Use the singular 2nd-person verb forms.
- **Action labels (buttons, menu items): imperative, second-person singular.** MS Amharic: "commands and menu items
  should be verbs in the imperative mood, second person, singular" (verified 2026-06-20), and GNOME matches: "ክፈት"
  (Open), "እንደገና ሰይም" (Rename), "ተወው" (Cancel/leave) (GNOME Nautilus, verified 2026-06-20). So the rule: **labels and
  user-facing instructions both use the singular imperative.** Confidence: high (MS and GNOME agree).

## Decision points

- **Script: Ge'ez/Ethiopic.** Covered as the headline flag above, including the font-readiness gate. Confidence: high.
- **Regional variant: one, `am` (`am-ET`).** Amharic is the working language of Ethiopia; no second national standard,
  no variant matrix. Confidence: high.
- **Gender / inclusive language (a genuine Amharic concern, tentative on the fix).** Amharic 2nd-person and verb forms
  are GENDERED (masculine "anta"/ verb forms vs feminine "anchi"/ forms). Addressing an unknown single user forces a
  gender choice - and unlike Slavic languages, there's no neutral plural escape because the chosen register is informal
  SINGULAR. Options: (a) default to masculine (common but not inclusive), (b) use plural forms for neutrality (but that
  contradicts the informal-singular norm), (c) rephrase impersonally where possible. This is a real tension between the
  informal-singular norm and gender-neutrality; recommendation: rephrase impersonally/nominally wherever a string would
  otherwise pick a gender, and push the residual cases to a native reviewer. Confidence: tentative - needs a native
  call.
- **Numerals: Western (0-9) vs Ge'ez numerals (high).** Amharic has its own Ge'ez numerals (፩፪፫…), but Western Arabic
  digits dominate in modern software and everyday use. Recommendation: Western digits via `Intl`; Ge'ez numerals are
  archaic for a file manager. Confidence: high.
- **Capitalization: not applicable.** The Ethiopic script has no case distinction, so the sentence-case rule is moot for
  Amharic text (it still applies to embedded Latin brand tokens). Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/am/` (GNOME Nautilus, Xfce
Thunar) and `am-ET/` (MS terminology, MS style guide; NO macOS) on 2026-06-20. Sources decide the term; Cmdr writes its
own value (MS copyrighted, GNOME/Xfce GPL, never copied verbatim). Without macOS, terms are `tentative` unless GNOME and
MS clearly agree.

- **open: `ክፈት`** · GNOME ("ክፈት"). Singular imperative. `high`.
- **rename: `እንደገና ሰይም`** · GNOME ("እንደገና ሰይም", lit. name again). `high`.
- **cancel: `ተወው`** · GNOME ("ተወው", leave it). `high`.
- **trash: `የማይፈለግ` (the unwanted) / `መጣያ`** · GNOME ("የማይፈለግ"); "መጣያ" (dump/bin) also common. Verify which reads best.
  `tentative`.
- **folder, file, search, volume, pane, tab, bookmark** · no clean GNOME match captured; defer to a native reviewer
  using the MS `am-ET` glossary. `tentative`.

Add terms as they come up; triangulate GNOME + the MS `am-ET` glossary, and record confidence. Native review is
mandatory given the thin sources.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand tokens
sit inside Ethiopic text; keep them Latin.

## Plurals

CLDR categories for `am`: `one`, `other` (verified with `new Intl.PluralRules('am')`). Only two forms. Note: Amharic's
`one` category includes 0 as well as 1 (CLDR groups 0 and 1 as "one" for Amharic), which is unusual - don't assume 0
falls into `other`. The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Punctuation.** Amharic traditionally uses Ethiopic punctuation: the word separator `፡` (though modern text often
  uses spaces), the full stop `።` (arat netib), and comma `፣`. Modern software commonly mixes Ethiopic and Western
  punctuation; defer the exact convention to a native reviewer, but the sentence-ending `።` is expected for full
  sentences.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` produce locale-correct output
  (Western digits for `am`). Note Ethiopia uses its own calendar; date display from `Intl` may need a reviewer's eye,
  but Cmdr's formatter layer owns this, not the strings.
- **Length.** Ethiopic is fairly compact per character; length is a low risk, but still overflow-check against the
  pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/am/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
