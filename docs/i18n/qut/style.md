# K'iche' (qut) translation style guide

Working notes for translating Cmdr into K'iche'. Read [`README.md`](../README.md) for how this fits the translation
process.

VERY LOW priority. K'iche' (also Quiché) is a Mayan language of highland Guatemala (~1M speakers). The pile has ONLY
Microsoft terminology for `qut-GT`; no macOS (Apple ships no K'iche' UI), no GNOME/Xfce. There is essentially no
consumer software-localization precedent beyond Microsoft's Guatemala terminology effort.

## Voice and tone

Friendly, concise, active, never alarmist. With almost no software precedent, prioritize plain clarity; every choice
needs a native reviewer.

## Formality

K'iche' uses Latin script (the Mayan-languages unified alphabet, ALMG standard, with the glottal-stop apostrophe ' as a
letter, e.g. "K'iche'"). It has formal/respectful registers but no software convention to anchor to. Use plain
imperative for actions. Confidence: tentative across the board.

## Decision points

### Resourcing / scope (the headline finding)

The real "decision point" is whether to localize at all. No Apple OS support, no open-source file-manager catalogs, only
Microsoft's `qut-GT` terminology. Recommendation: DEPRIORITIZE; do not attempt without a committed native K'iche'
reviewer. If pursued, Microsoft `qut-GT` is the only triangulation source, so confidence stays tentative on every term.
Confidence: high that this is low priority.

### Script and orthography

Latin script, ALMG (Academia de Lenguas Mayas de Guatemala) unified orthography. The apostrophe-as-glottal-stop is a
real letter, not punctuation, do NOT confuse it with the ICU escape apostrophe. CAUTION: K'iche' words containing `'`
will collide with ICU's apostrophe-escaping; per the template's ICU rule, every literal `'` in a value must be doubled
(`''`), which matters a lot here since the language uses `'` constantly. Recommendation: ALMG orthography; be vigilant
about ICU apostrophe doubling. Confidence: high on the orthography, high on the ICU caution.

### Numerals

Use Western digits (Mayan vigesimal numerals are not used in modern software). Let `Intl` handle formatting. Confidence:
high.

## Terminology and glossary

Defer; only Microsoft qut-GT terminology available, every term tentative pending native review.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `qut`: `one`, `other`. Two forms.

## Decisions to confirm with David

- Is K'iche' in scope at all? Recommend no for launch: no Apple reference, near-zero software precedent, requires a
  dedicated native reviewer. The ICU apostrophe-doubling collision (language uses `'` heavily) is a real technical
  gotcha if it does proceed.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/qut/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
