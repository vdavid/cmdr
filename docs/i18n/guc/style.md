# Wayuu (guc) translation style guide

Working notes for translating Cmdr into Wayuu (Wayuunaiki). Read [`README.md`](../README.md) for how this fits the
translation process.

This is the language base (`guc`), Wayuunaiki, the Arawakan language of the Wayuu people of the La Guajira peninsula
(northern Colombia and northwestern Venezuela). It's an indigenous, low-resource language for software purposes.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register as closely as the language's tech vocabulary
allows. Because there's no established software register for Wayuunaiki, lean toward plain, concrete phrasing.

## Formality

**No established UI formality convention exists.** Wayuunaiki has no software T/V tradition to inherit. Address the user
directly and plainly. This is a David- and native-reviewer call, not something the sources settle. Confidence:
tentative. FLAG.

## Decision points

**This is a very-low-resource locale, and that's the headline finding.** The reference pile has ONLY a Microsoft
terminology file (`microsoft-terminology/WAYUU.tbx`, ~2,800 entries) for Wayuu (verified 2026-06-20): no macOS, no
Microsoft style guide, no GNOME/Xfce catalog. So there's a vendor term list but no tone/formality guidance and no
file-manager catalog to triangulate against. The Microsoft terminology (from Microsoft's indigenous-language program) is
the one anchor, and even it won't cover every file-manager term. Treat Wayuu as a low-priority, native-review-gated
locale: an agent draft here is genuinely a first pass that a Wayuunaiki speaker must rebuild. Confidence: confirmed
(about the coverage).

- **Script: Latin only.** Wayuunaiki is written in the Latin alphabet (the Aguilar / standardized orthography, with
  letters like ü and the apostrophe-marked glottal/saltillo). Use the standardized orthography; never romanize beyond
  it. Confidence: tentative (orthography variants exist among communities; a native reviewer should settle which). FLAG.
- **Terminology gaps are the core problem.** Many computing concepts (folder, tab, volume, SMB, transfer) have no
  settled Wayuunaiki term. The Microsoft terminology has some (file = "anaajaalaa", folder = "katpeeta", copy =
  "ashataa", delete = "awasütaa", cancel = "oo'ulawaa"), but expect to leave brand/technical tokens in English/Spanish
  and flag many terms for a native reviewer. Don't invent coinages silently. Confidence: tentative.
- **Gender / inclusive language**: not a known UI concern for Wayuunaiki at this stage; defer to native review.
  Confidence: tentative.
- **Length**: unknown; agglutinative morphology can lengthen words. Overflow-check against the pseudolocale once any
  real strings exist. Confidence: tentative.

## Terminology and glossary

| English term | Wayuu      | Notes                                                |
| ------------ | ---------- | ---------------------------------------------------- |
| Copy         | ashataa    | Microsoft terminology                                |
| Delete       | awasütaa   | Microsoft terminology                                |
| Cancel       | oo'ulawaa  | Microsoft terminology (note the saltillo apostrophe) |
| file         | anaajaalaa | Microsoft terminology                                |
| folder       | katpeeta   | Microsoft terminology (Spanish-derived "carpeta")    |
| Move         | (confirm)  | not found in Microsoft terminology sample            |
| trash        | (confirm)  | not found; needs native review                       |

(All rows: tentative, native-review-gated. The Microsoft terms are the best available anchor, not confirmed UI usage.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. Given the
vocabulary gaps, expect more English/Spanish passthrough than in a well-resourced locale.

## Plurals

Wayuu CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('guc')`, 2026-06-20). The
`desktop-i18n-plural` check enforces coverage. A native reviewer should confirm how counted nouns actually inflect.
Confidence: confirmed (categories); tentative (grammar).

## Notes and decisions

- **The apostrophe (saltillo) is a real letter** in Wayuunaiki orthography (glottal stop), e.g. "oo'ulawaa". This
  collides with ICU's apostrophe escaping: double EVERY apostrophe in ICU values (`'` becomes `''`), which matters a lot
  here because the saltillo is frequent. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).
- **Numbers and dates come from the formatter layer.** Never hardcode.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.

## Decisions to confirm with David

- **Priority and feasibility.** Wayuu has only a Microsoft term list, no style/tone source, no file-manager catalog. An
  agent draft is a genuine first pass needing full native rebuild. Confirm whether to attempt it at all before the
  better-anchored languages. FLAG.
- **Orthography variant and formality register** both need a Wayuunaiki native reviewer. FLAG.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/guc/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
