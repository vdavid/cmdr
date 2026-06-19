# Walloon (wa) translation style guide

Working notes for translating Cmdr into Walloon (walon). Read [`README.md`](README.md) for how this fits the translation
process.

This is the language base (`wa`), a Romance language of southern Belgium written in Latin script. The only reference in
the pile is a GNOME catalog (`wa`, ~52% translated); no macOS, no Microsoft. It's a low-resource, regional-minority
target.

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. Walloon has little modern software UI to
anchor a house tone; aim for plain, modern Walloon. Keep error and crash copy calm and factual. The GNOME catalog is the
nearest register reference.

## Formality

Walloon, like French, distinguishes informal **ti/vos** address. Following the French convention (Cmdr's `fr` uses the
polite "vous"; see [`fr-style.md`](fr-style.md)), **use the polite "vos"** form throughout for an unknown adult user.
For UI action labels, the GNOME catalog uses bare imperative/short verb forms (e.g. `Drovi` Open, `Rinoncî` Cancel);
follow that, kept short. Confirm the register with David / a native reviewer.

## Decision points

### Orthography: rifondou (unified) vs Feller (traditional dialectal)

- The choice: this is Walloon's defining localization decision. Two competing orthographies exist: rifondou walon (the
  modern pan-dialectal standard, designed to be dialect-neutral in spelling) and the older Feller system (phonetic,
  tied to a writer's local dialect). They produce visibly different spellings of the same words.
- Majors: none in the pile to arbitrate (no macOS, no Microsoft Walloon). The GNOME catalog uses Walloon Latin spelling
  (e.g. `Rinoncî` Cancel, `Grandeu` Size, `Drovi` Open) but isn't a clean single-standard reference.
- Recommendation: target rifondou walon, the unified standard built for exactly this (one spelling that serves all
  dialects), which is the right choice for a single shipped UI that can't pick a region's dialect. Confirm with David;
  this is the one call that shapes every string and a native Walloon community has strong views on it.
- Confidence: tentative (it's a genuine community-political choice, David-only).

### Region / dialect variant

- The choice: Walloon spans several dialect areas (Liège, Namur, Charleroi, etc.) with real lexical and phonetic
  differences. Whether to target one dialect or a dialect-neutral standard.
- Majors: none in the pile.
- Recommendation: don't split region variants; ship a single `wa` base in the dialect-neutral rifondou orthography (the
  decision point above), which is precisely how rifondou sidesteps the dialect problem.
- Confidence: tentative (tied to the orthography call).

### Special characters

- The choice: Walloon Latin uses circumflex and other diacritics (`Rinoncî`, `Drovi`) and the digraph å/xh in rifondou.
- Recommendation: keep all diacritics and rifondou digraphs in NFC; verify the app font renders å and any circumflexed
  vowels.
- Confidence: high.

### Gender

- The choice: Walloon is a Romance language with grammatical gender (masculine/feminine nouns and agreement), so adjective
  and participle agreement matters around inserted values.
- Recommendation: as with French, structure sentences so an inserted `{name}`/`{path}` doesn't force a gendered
  agreement the template can't know; prefer agreement-neutral phrasings. No special inclusive-form policy beyond that.
- Confidence: medium.

## Terminology and glossary

Sparse (only a half-translated GNOME catalog); terms are tentative and need native review, and many file-manager terms
(pane, tab, volume, transfer, trash) aren't in the source at all and will be coined.

| English term | Walloon | Notes |
| ------------ | ------- | ----- |
| Open | drovi | GNOME (tentative) |
| Cancel | rinoncî | GNOME (tentative) |
| Name | no | GNOME (tentative) |
| Size | grandeu | GNOME (tentative) |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

Walloon CLDR plural categories: `one`, `other` (run `new Intl.PluralRules('wa').resolvedOptions().pluralCategories` to
confirm; the GNOME header omits a Plural-Forms line, but the 2-form Romance pattern applies). Like French, treat the
small-number boundary per the category, and write both branches as full native forms.

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings (Walloon uses apostrophes in elisions, so this matters);
  normal apostrophes in `errors.*`.
- **Unicode normalization**: keep circumflexed vowels and rifondou digraphs (å, xh) in NFC.

## Decisions to confirm with David

- Orthography: rifondou (recommended) vs Feller, the one community-political call that shapes every string.
- Whether to target dialect-neutral standard (recommended) vs a specific dialect.
- Whole glossary is tentative (one half-translated source); needs a native Walloon reviewer, and most file-manager terms
  will be coined fresh.
