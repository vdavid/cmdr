# Low German (nds) translation style guide

Working notes for translating Cmdr into Low German (Plattdüütsch / Low Saxon). Read [`README.md`](README.md) for how
this fits the translation process. `nds` is a separate West Germanic language, not a dialect of Standard German (`de`);
its vocabulary, spelling, and grammar diverge from High German across the sentence.

## Voice and tone

Cmdr's Low German voice mirrors its English one: friendly, concise, active, and never alarmist. The only file-manager
reference for `nds` is GNOME Nautilus (Tier 3); it reads plain and direct, which suits Cmdr's register.

- Address the user informally as **du**, the natural form in Low German (the language is colloquial and regional by
  nature; a formal register is rarely written).
- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed". Rewrite around
  what happened.
- Never machine-translate from High German by find-replace: `nds` uses `un` not `und`, `nich` not `nicht`, `Datei`
  vs the GNOME catalog's choices, etc. The forms differ systematically.

## Formality

- **Second person: informal `du`, always.** Low German is a spoken/regional language with a strong informal default;
  a formal `Se`/`Ji` paradigm exists but is not used in software. Recommendation: `du` everywhere. Confidence: high.
- **UI actions use the imperative**, matching the GNOME Nautilus Low German catalog ("Avbreken" for Cancel).
  Recommendation: imperative for buttons and menu items. Confidence: medium (single source).

## Decision points

- **Low-resource language; very thin reference base.** `nds` has only a GNOME catalog (no macOS, no Microsoft
  terminology or style guide). Apple and Microsoft do not ship Low German; it survives in software only through
  free-software community catalogs (GNOME, KDE, LibreOffice). Recommendation: treat every term as `tentative` unless the
  GNOME catalog confirms it, and lean hard on native human review. Confidence: high (about the gap itself). Flag for
  David: `nds` is a deliberate community-goodwill locale, not a coverage necessity; nearly all speakers also read
  Standard German.
- **Spelling standard.** Low German has no single binding orthography; SASS (Sass'sche Schrievwies) is the most common
  modern convention and what the GNOME catalog broadly follows. Recommendation: follow SASS and the GNOME catalog's
  spelling; lock choices in the glossary. Confidence: medium. Flag for David: spelling is genuinely unsettled across
  speakers; a native reviewer's regional convention may differ.
- **Regional variation.** `nds` spans Northern Germany and the eastern Netherlands (where it overlaps with `nl`-influenced
  Low Saxon). The GNOME catalog is German-side Low German. Recommendation: target German-side Plattdüütsch; don't try to
  serve Dutch Low Saxon in the same build. Confidence: medium.
- **Formality: informal `du`.** Covered above. Confidence: high.
- **Letters and special characters.** Uses the German letters `ä`, `ö`, `ü`, `ß` (and the GNOME catalog uses `ö` in
  "Papierkörv"). Never transliterate to `ae`/`oe`/`ue`/`ss`. Confidence: high.
- **Inclusive/gendered language.** Grammatical gender on nouns; no he/she issue in generic UI copy (user addressed as
  `du`). No special handling. Confidence: medium (low-stakes).

## Terminology and glossary

From GNOME Nautilus Low German (Tier 3); treat as `tentative` pending native review.

| English term | Low German | Notes |
| ------------ | ---------- | ----- |
| trash | Papierkörv | GNOME; the location noun |
| cancel | Avbreken | GNOME; imperative |
| file | Datei | tentative; confirm against native usage |
| folder | Ornner | tentative; High German `Ordner` -> nds `Ornner` |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `nds`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('nds').resolvedOptions().pluralCategories`). Same two-category shape as English; every plural
message needs both branches. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Letters: German `ä`, `ö`, `ü`, `ß`; never transliterate.
- Spelling: follow SASS and the GNOME catalog; this is genuinely unsettled, so flag any hard call to David and lean on
  native review.
- Translate fresh; never convert from High German by find-replace.
- Every glossary term here is `tentative` until a native reviewer confirms it: the reference base is one catalog.
