# Maithili (mai) translation style guide

Working notes for translating Cmdr into Maithili. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Maithili.

Sources for `mai/`: a Microsoft style guide PDF (Tier 2, tone/grammar) and an old, ~73%-translated GNOME Nautilus
catalog (`mai/gnome-nautilus/nautilus.po`, 2008, Devanagari). No macOS (Tier 1), no Microsoft terminology TBX. Term
authority is weaker than for de/sv/fr; native review is essential before ship.

## Voice and tone

Friendly, concise, active, calm. Microsoft's Maithili voice ("warm and relaxed, crisp and clear, ready to lend a hand,"
short sentences, no literal translation) aligns well with Cmdr's voice. Error messages stay calm and actionable.

## Formality

Mid-honorific `अहाँ` tier (see Decision points). Use the matching verb inflections (the `-ू` imperative endings: खोलू
"open", बनाबू "create", मेटाबू "delete"). Microsoft mandates `अहाँ` and forbids तू/तों/ओ; the 2008 Nautilus catalog
independently uses the same tier. Avoid both तों (too familiar, can read as disrespectful) and the high-honorific अपने
(too distant for an app).

## Decision points

Script, Devanagari (decided):

- Options: Devanagari (modern computing standard) vs Tirhuta/Mithilakshar (historical/cultural, zero software-UI
  presence). Microsoft's mai guide and the Nautilus catalog are both Devanagari.
- Recommendation: Devanagari only; don't offer Tirhuta. Sub-rule: NO nuqta, the MS guide says the nuqta diacritic (used
  in Hindi for borrowed sounds) is strictly avoided in Maithili. A concrete divergence from Hindi to bake in.
  Confidence: high.

Honorific register, pick the `अहाँ` mid tier (flag for David):

- Maithili verb agreement encodes honorificity: roughly non-honorific (तों/तू), mid-honorific (अहाँ), high-honorific
  (अपने). The choice propagates into every imperative button, so consistency across all action strings matters.
- Both sources agree on `अहाँ`: Microsoft mandates it (and forbids तू/तों/ओ); Nautilus independently uses the अहाँ-tier
  imperatives (खोलू, बनाबू, मेटाबू).
- Recommendation: use the `अहाँ` mid tier consistently, respectful without being stiff, and the right fit for Cmdr's
  friendly-but-respectful voice. Confidence: high on the tier (two sources agree); flagged for David because it's a
  genuine register decision with cultural weight and ideally wants a native reviewer to confirm tone.

Tech vocabulary, transliterated loans + Sanskritic, with English acronyms kept (medium confidence):

- Nautilus uses फाइल (file, transliterated English), फ़ोल्डर (folder), रद्द/रद्दी (cancel/trash, via Hindi), and
  Sanskritic terms (प्रबंधन, वरीयता). MS keeps well-known English acronyms in Roman (PIN, ID, Wi-Fi), glosses others on
  first use, and translates "&" as आ/आओर.
- Recommendation: mirror this, established transliterated loans for core nouns, common Sanskritic terms where already
  used, keep widely-known acronyms in Roman, no nuqta, translate "&". With no macOS/TBX tiebreaker, defer fine term
  choices to a native reviewer. Confidence: medium (two sources, one of them 2008-old).

Near-absence of major-vendor localization (sets expectations):

- Microsoft has a mai style guide (and historically a LIP). Apple does NOT localize into Maithili. Google: no general UI
  localization. Spotify's Indic expansion didn't include it. Netflix: no Maithili UI. Cmdr would be an early mover; term
  confidence stays tentative/high until native review. Confidence: high.

## Terminology and glossary

From the 2008 Nautilus catalog (loose reference; needs native confirmation). Format:
`English → chosen · source · confidence`.

- file → फाइल · GNOME · tentative
- folder → फ़ोल्डर · GNOME · tentative
- open → खोलू · GNOME (अहाँ-tier imperative) · tentative
- create folder → फ़ोल्डर बनाबू · GNOME · tentative
- delete → मेटाबू · GNOME · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens, and well-known English acronyms in Roman (PIN, ID, Wi-Fi). Enforced by
`desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

## Plurals

CLDR categories: `one`, `other` (rule n != 1), confirmed by the Nautilus header. The `desktop-i18n-plural` check
requires every plural message to cover the categories this language needs.

## Notes and decisions

- Capitalization: not applicable in Devanagari (single letterform); the English upper/lower distinction doesn't exist
  (MS guide §5).
- Punctuation: full stop is the Purn Viram (।); comma, "!", and "?" use the English symbols (MS guide §6).
- Numbers: numerals for most counts; non-breaking space between number and unit; Indian date format DD/MM/YYYY (MS guide
  §8-9). These come from the formatter layer, never hardcode.
- Em dash: not prevalent in Maithili (MS guide says avoid), aligns with Cmdr's no-em-dash rule.
- Record case-by-case rulings here.

## Decisions to confirm with David

- The honorific tier: confirm `अहाँ` mid-honorific is right for the app's voice (cultural weight; native review ideal).
- Tech-vocabulary level (transliterated vs Sanskritic) per term, with a native reviewer, since there's no macOS/TBX
  tiebreaker.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in `docs/guides/i18n-translation.md` and
`apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/mai/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
