# Norwegian Nynorsk (nn) translation style guide

Working notes for translating Cmdr into Norwegian Nynorsk. Read [`README.md`](../README.md) for how this fits the
translation process. `nn` is the Nynorsk written standard; Bokmål is the separate [`nb`](../nb/style.md) locale. They are
two written standards of the same spoken language, not dialects, and must not be mixed within one build.

## Voice and tone

Cmdr's Nynorsk voice mirrors its English one: friendly, concise, active, and never alarmist. Nynorsk UI copy from
GNOME and Xfce (Nynorsk has a strong free-software tradition) is plain and direct, so this register is native, not a
stretch.

- Address the user informally as **du** (lowercase), universal in modern Norwegian software.
- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed". Rewrite around
  what happened and what to do rather than reaching for `mislukkast`, just as the `nb` guide does.
- Drop English filler ("successfully"); a Norwegian sentence states the outcome without it. Avoid `ver venleg`
  ("please") in terse actions.
- Nynorsk has its own vocabulary and forms distinct from Bokmål (`ikkje` not `ikke`, `kva` not `hva`, `eg` not `jeg`,
  `frå` not `fra`). Never machine-translate Bokmål to Nynorsk by find-replace; the forms differ across the sentence.

## Formality

- **Second person: informal `du`, always.** The formal `De` paradigm is dead in modern Norwegian and not recommended
  for software. Apple, Microsoft, and GNOME all use `du`. No register decision to make.
- **UI actions use the imperative**, matching GNOME Nynorsk and Xfce: "Kopier", "Flytt", "Slett", "Opna", "Lim inn",
  "Endra namn", "Avbryt". Do NOT use the infinitive for buttons.

## Decision points

- **Nynorsk is a minority written standard; confirm it's worth shipping.** Bokmål is the form for ~85-90% of Norway and
  the universal software default; Nynorsk is the active written standard for ~10-15%, with strong institutional and
  free-software backing (it ships in Windows, Office, GNOME, LibreOffice). Apple does NOT ship a Nynorsk macOS
  (Norwegian = Bokmål only there), so the Tier-1 source is absent for `nn`. Recommendation: ship `nn` only as a
  deliberate second Norwegian locale after `nb`, not instead of it. Confidence: high. Flag for David: whether a Nynorsk
  build is in scope at all, given Bokmål covers nearly every Norwegian user.
- **Formality: informal `du`.** Covered above. Recommendation: `du` everywhere, lowercase. Confidence: high.
- **Imperative, not infinitive, for buttons.** GNOME Nynorsk and Xfce Nynorsk use the imperative. Recommendation:
  imperative throughout. Confidence: high.
- **Samanskriving (compound spacing).** Like Bokmål, Nynorsk writes compounds as ONE word where English uses two:
  "filnamn" (file name), "målmappe" (destination folder), "søkefelt" (search field). Splitting them is the most common
  and most visible Norwegian localization error. Recommendation: compound by default; rephrase rather than split when
  unwieldy. Confidence: high. Deserves a dedicated human review pass.
- **Capitalization: sentence case, lighter than English.** Only the first word and proper nouns capitalized; days,
  months, and languages lowercase ("måndag", "januar", "norsk"). Recommendation: sentence case. Confidence: high.
- **Letters and quotation marks.** `æ`, `ø`, `å` are full letters; never transliterate to `ae`/`oe`/`aa`. Quotation
  marks are guillemets «like this», matching Apple and Microsoft Norwegian. Numbers use a space thousands separator and
  comma decimal; `Intl` handles this at runtime. Confidence: high.
- **Inclusive/gendered language.** Grammatical gender on nouns but no he/she issue in generic UI copy (user addressed as
  `du`). No special handling. Confidence: medium (low-stakes).

## Terminology and glossary

Confirmed against GNOME Nynorsk and Xfce Nynorsk (Tier 3) plus the `nb` guide where the forms coincide. Extend as
strings come up.

| English term | Norwegian Nynorsk | Notes |
| ------------ | ----------------- | ----- |
| file | fil | |
| folder | mappe | |
| copy | kopier | imperative |
| move | flytt | imperative |
| delete | slett | imperative |
| trash | papirkorg | GNOME Nynorsk; the location noun |
| rename | endra namn | Nynorsk `namn`, not Bokmål `navn` |
| paste | lim inn | |
| cut | klipp ut | |
| open | opna | Nynorsk `opna`, not Bokmål `åpne` |
| cancel | avbryt | GNOME Nynorsk |
| tab | fane | UI tab, not the key |
| settings | innstillingar | Nynorsk `-ar` plural |
| file name | filnamn | one word (samanskriving) |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `nn`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('nn').resolvedOptions().pluralCategories`; GNOME Nynorsk uses `nplurals=2; plural=(n!=1)`). Same
two-category shape as English; every plural message needs both branches. Noun gender interacts with the count word and
any adjective, so write each branch as a full natural phrase. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Quotation marks: guillemets «…».
- Punctuation and capitalization: sentence case; lowercase days, months, and language names.
- Letters: `æ`, `ø`, `å` are full letters; never transliterate.
- Numbers: comma decimal mark, space thousands separator; `Intl` handles formatting at runtime.
- Nynorsk forms differ systematically from Bokmål (`ikkje`/`kva`/`eg`/`frå`/`namn`/`-ar` plurals); translate fresh, never
  convert from `nb`.
- Dedicate one human review pass to samanskriving (compound spelling).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/nn/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
