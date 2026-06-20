# Lao (lo) translation style guide

Working notes for translating Cmdr into Lao. Read [`README.md`](../README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into Lao.

Sources for `lo/`: Microsoft terminology (`LAO.tbx`) and a near-complete Xfce Thunar catalog
(`lo/xfce-thunar/thunar.po`, ~1,030 of ~1,074 strings, `nplurals=1`). No macOS folder, Apple ships no Lao Finder strings
(see Decision points).

## Voice and tone

Friendly, concise, active, calm, neutral-polite. Lao has politeness particles and register; keep software-typical
neutral-polite phrasing rather than particle-heavy conversational copy. Error messages stay calm and actionable.

## Formality

Neutral-polite register, concise. A Microsoft Lao style guide PDF exists (Microsoft's localization-style-guides site)
but isn't in the local pile, so detailed register guidance is thin here. Defer particle-level decisions to a native
reviewer; pull the MS Lao style guide for them.

## Decision points

Script rendering + line-breaking with NO word spaces (the dominant technical pitfall):

- Lao is an abugida with stacked/combining vowel marks above and below the base consonant, and it writes running text
  with NO spaces between words (spaces mark clause/sentence/list boundaries only). Word and line breaking need
  dictionary-based segmentation.
- ICU ships dictionary break iterators for Lao (and Thai/Khmer/Burmese), breaking at phonetic-syllable boundaries, and
  activates automatically when Lao text is detected. Apple, Microsoft, and browsers all rely on ICU segmentation. Naive
  break-on-space or grapheme truncation breaks mid-syllable and corrupts stacked vowel marks.
- Recommendation: rely on the platform/webview ICU line-breaker (`line-break`/`word-break` defaults), never hand-roll
  space-based wrapping; verify CSS truncation/ellipsis doesn't split combining marks. Confidence: high.

Apple ships no Lao macOS UI:

- Lao sits below the separator line in macOS Language & Region (keyboard since Catalina, fonts bundled, but not a
  supported interface language). So there's no Apple Finder reference for Lao terms.
- Recommendation: anchor on Thunar (file-manager domain) cross-checked with Microsoft terminology; mark the few
  conflicts (copy, a folder-spelling variant) tentative, and gate the locale on native review before shipping.
  Confidence: high on the absence, medium on term picks.

Numerals, Lao vs Arabic digits:

- Lao has its own digits (໐໑໒໓…) but Arabic/Latin digits dominate modern Lao tech UI; CLDR defaults lo to `latn`.
- Recommendation: use Arabic numerals (CLDR `latn` default) in counts, sizes, and dates; don't force Lao digits.
  Confidence: high.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. M = Microsoft terminology, T = Thunar. Where they disagree, weight T
for file-manager context but mark tentative.

- file → ໄຟລ໌ · M, T agree · high
- folder → ໂຟລເດີ · M (T spells ໂຟນເດີ, minor ນ/ລ variant) · tentative
- copy → ສຳເນົາ (T) · M uses ກ່າຍ, sources disagree; T's ສຳເນົາ is more standard · tentative
- move → ຍ້າຍ · T · high
- cancel → ຍົກເລີກ · M, T agree · high
- delete → ລຶບ · M, T agree · high
- open → ເປີດ · M, T agree · high
- search → ຄົ້ນຫາ · M, T agree · high
- send → ສົ່ງ · M · high
- rename → ປ່ຽນຊື່ · T · high
- settings → ການຕັ້ງຄ່າ · M · high
- trash / recycle bin → ຖັງຂີ້ເຫຍື້ອ · M, T agree (tiny vowel-mark diff) · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR category: `other` only, Lao makes no grammatical count distinction. Single-form messages; no `{count, plural, …}`
branching is needed (confirmed by Thunar's `nplurals=1; plural=0`). The `desktop-i18n-plural` check requires every
plural message to cover the categories this language needs (here, just `other`).

## Notes and decisions

- Don't insert spaces to "help" wrapping, spaces change meaning in Lao. Let ICU segment.
- Watch CSS truncation: ellipsis must land on a syllable boundary, never mid-combining-mark.
- Numbers and dates come from the formatter layer (Arabic digits). Never hardcode.
- Record case-by-case rulings here.

## Decisions to confirm with David

- copy → ສຳເນົາ vs ກ່າຍ, and the folder spelling variant (ໂຟລເດີ vs ໂຟນເດີ): native reviewer to settle.
- Register / politeness particles: pull the MS Lao style guide and have a native reviewer set the level.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in
[`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/lo/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
