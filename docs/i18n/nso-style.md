# Northern Sotho / Sepedi (nso) translation style guide

Working notes for translating Cmdr into Northern Sotho (Sepedi), one of South Africa's official languages. Read
[`README.md`](README.md) for how this fits the translation process. Sepedi is written in the Latin script (with no
special diacritic letters beyond `š`); there is no script decision. References: Microsoft terminology (`nso-ZA`,
Tier 2) and GNOME Nautilus (`nso`, Tier 3).

## Voice and tone

Cmdr's Sepedi voice mirrors its English one: friendly, concise, active, and never alarmist. The reference base is thin
(Microsoft terminology and a partial GNOME catalog), so lean on native human review for tone.

- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Drop English filler ("successfully").
- Sepedi is part of the Sotho-Tswana group and is close to Sesotho (`st`) and Setswana (`tn`); translate fresh for
  Sepedi rather than borrowing a Sesotho/Setswana string, since the forms differ.

## Formality

- **Second person: standard polite address.** Sepedi (like the other Bantu languages) handles respect mainly through
  noun-class concords and plural address rather than a French-style T/V split. Software-style copy uses ordinary polite
  second person. Recommendation: standard second person; defer the exact register to native review. Confidence: medium.
- **UI actions use the imperative**, matching the GNOME/Microsoft references ("Khansela" cancel, "bula" open, "kopiša"
  copy). Recommendation: imperative for buttons and menu items. Confidence: medium-high.

## Decision points

- **Low-resource language; thin reference base.** `nso` has Microsoft terminology and a partial GNOME catalog only (no
  macOS; Apple ships no Sepedi). It IS an official South African language with some institutional backing, but digital
  coverage is sparse. Recommendation: treat most terms as `tentative`, prefer the Microsoft term where it exists
  (highest authority here), and lean hard on native human review. Confidence: high (about the gap). Flag for David:
  nearly all Sepedi speakers also read English or Afrikaans; `nso` is a goodwill locale, not a coverage necessity.
- **Script: Latin only.** Sepedi uses the Latin alphabet with `š` (s-caron) as its one special letter; no script
  decision. Never substitute `sh` for `š`. Recommendation: Latin with `š`. Confidence: high.
- **Formality.** Covered above. Confidence: medium.
- **Anglicism/loanword handling.** The references freely adapt English computing terms into Sepedi orthography
  ("foltara" folder, "faele" file). Recommendation: follow this adapted-loanword pattern where the references do; use a
  native term where one is well-attested. Confidence: medium.
- **Term length.** Bantu verb forms with concords and affixes can run long; watch for UI overflow (the pseudolocale
  catches this). Recommendation: prefer the shortest natural phrasing. Confidence: medium.
- **Inclusive/gendered language.** Sepedi has noun classes but NO grammatical gender and no gendered pronouns; generic
  UI copy is naturally gender-neutral. No special handling. Confidence: high.

## Terminology and glossary

From Microsoft terminology (Tier 2) and GNOME Nautilus (Tier 3). Treat as `tentative` pending native review.

| English term | Sepedi | Notes |
| ------------ | ------ | ----- |
| folder | foltara | Microsoft; adapted loanword |
| file | faele | Microsoft; adapted loanword |
| copy | kopiša | Microsoft |
| move | šuthiša | Microsoft |
| delete | tloša | Microsoft |
| open | bula | Microsoft |
| cancel | khansela | GNOME/Microsoft |
| trash | ditlakala | GNOME (="litter/rubbish"); the location noun |
| rename | thea ka leswa | GNOME ("name anew") |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `nso`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('nso').resolvedOptions().pluralCategories`; the GNOME catalog uses `nplurals=2; plural=n>1`, i.e.
`one` covers 0 and 1). Every plural message needs both branches. Noun-class concords change with the counted noun, so
write each branch as a full natural phrase. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Script: Latin with `š`; never substitute `sh`.
- Gender: no grammatical gender; copy is naturally neutral.
- Translate fresh for Sepedi; don't borrow Sesotho/Setswana strings.
- Reference base is thin (Microsoft terminology + partial GNOME); most terms are `tentative` until native review.
- Watch UI overflow: Bantu concord forms run long.
