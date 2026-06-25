# Venetian (vec) translation style guide

Working notes for translating Cmdr into Venetian (Vèneto). Read [`README.md`](../README.md) for how this fits the
translation process. `vec` is a Romance language of northeastern Italy, distinct from Standard Italian (`it`); don't
route translation through Italian.

## Voice and tone

Cmdr's Venetian voice mirrors its English one: friendly, concise, active, and never alarmist. The only file-manager
reference for `vec` is Xfce Thunar (Tier 3); it reads plain and direct, which suits Cmdr's register.

- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Drop English filler ("successfully"); Venetian states the outcome without it.
- Translate fresh from English, not by adapting Italian; Venetian vocabulary and forms differ (`file`/`fiłe`, `spostar`
  not `spostare`, the `ł` grapheme).

## Formality

- **Second person: informal `ti`** is the natural default for a regional/colloquial language like Venetian. A formal
  `Vu`/`Eła` exists but is rarely used in software-style copy. Recommendation: `ti`. Confidence: medium (single Tier-3
  source). Flag for David if a more formal register is wanted.
- **UI actions use the infinitive**, matching the Xfce Venetian catalog ("Copiar", "Spostar", "Renomenar") and the
  broader Romance free-software convention. Recommendation: infinitive for buttons and menu items. Confidence: high
  (consistent in the catalog).

## Decision points

- **Spelling standard is unsettled.** Venetian has no single binding orthography. The most visible modern convention is
  the "Grafia Veneta Unitaria" (and the related DECA / Talian-influenced spellings), which uses the grapheme `ł` (an l
  with stroke) for the variable "evanescent l" sound (seen in the Xfce catalog's "Busołoto" for Trash). Other writers
  drop it. Recommendation: follow the Xfce catalog's GVU-style spelling with `ł`; lock choices in the glossary.
  Confidence: medium. Flag for David: spelling is genuinely contested among Venetian speakers; a native reviewer may
  prefer a simpler `l`-only convention.
- **Regional variant.** Venetian spans several local varieties (Venesian/lagoon, Padovan-Vicentino-Polesan, Trevisan,
  Veronese, plus diaspora Talian in Brazil). The Xfce catalog targets a general written Vèneto. Recommendation: target
  general written Venetian, not a single city's variety. Confidence: medium.
- **Low-resource caveat.** No macOS, no Microsoft (Apple and Microsoft don't ship Venetian). Sole reference is one Xfce
  catalog. Recommendation: treat most terms as `tentative` and lean on native human review. Confidence: high (about the
  gap). Flag for David: `vec` is a community-goodwill locale; nearly all speakers also read Italian.
- **Formality: informal `ti`.** Covered above. Confidence: medium.
- **Buttons: infinitive, not imperative.** Covered above. Confidence: high.
- **Letters and special characters.** GVU spelling uses `ł` and `x` (for a voiced-s sound, e.g. "xe"); keep diacritics
  on `à è ò` etc. Never strip `ł` to `l` silently if following GVU. Confidence: medium.
- **Inclusive/gendered language.** Gendered grammar; generic UI copy avoids the issue via `ti` and infinitives. No
  special handling beyond neutral role nouns where possible. Confidence: medium.

## Terminology and glossary

From Xfce Thunar Venetian (Tier 3); treat as `tentative` pending native review.

| English term | Venetian  | Notes                                                     |
| ------------ | --------- | --------------------------------------------------------- |
| file         | file      | Xfce kept the English word; confirm vs native "schedario" |
| copy         | copiar    | infinitive                                                |
| move         | spostar   | infinitive                                                |
| rename       | renomenar | infinitive                                                |
| trash        | busołoto  | Xfce; GVU spelling with `ł`                               |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR plural categories for `vec`: **`one`**, **`many`**, and **`other`** (confirmed via
`new Intl.PluralRules('vec').resolvedOptions().pluralCategories`). This is THREE categories, more than English's two:
`many` is the modern CLDR compound/large-number category Venetian shares with Italian. Every plural message must cover
all three branches the check needs. Note the older Xfce catalog predates this and used only two forms (`nplurals=2`);
CLDR/`Intl` is authoritative, so write `one`/`many`/`other`. The `desktop-i18n-plural` check requires the full set.

## Notes and decisions

- Spelling: follow the Xfce catalog's GVU-style convention (with `ł`); genuinely contested, so flag hard calls and lean
  on native review.
- Buttons: infinitive (Romance convention), not imperative.
- Translate fresh from English; never adapt from Italian.
- Numbers: comma decimal mark, dot/space thousands separator (Italian-style); `Intl` handles formatting at runtime.
- Sole reference is one Xfce catalog; most terms are `tentative` until a native reviewer confirms.
- Plurals are three-category (`one`/`many`/`other`) per CLDR, unlike the two-category Romance languages above.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/vec/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
