# Occitan (oc) translation style guide

Working notes for translating Cmdr into Occitan. Read [`README.md`](../README.md) for how this fits the translation
process. `oc` is a Romance language of southern France (and parts of Italy and Spain); it has a strong free-software
tradition, so GNOME and Xfce are full and consistent references.

## Voice and tone

Cmdr's Occitan voice mirrors its English one: friendly, concise, active, and never alarmist. The GNOME Nautilus and Xfce
Thunar Occitan catalogs (Tier 3) are well-maintained and plain, which suits Cmdr's register.

- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Drop English filler ("successfully"); Occitan states the outcome without it.
- Occitan vocabulary is close to Catalan and to southern French usage but is its own language; translate fresh, don't
  route through French.

## Formality

- **Second person: informal `tu`** is the natural software default and what GNOME/Xfce Occitan use. Occitan has a formal
  `vos`, but free-software UI copy is informal. Recommendation: `tu`. Confidence: medium (community-catalog evidence, no
  Tier-1/2 source). Flag for David if a more formal register is wanted.
- **UI actions use the infinitive**, matching GNOME/Xfce Occitan and the broader Romance free-software convention:
  "Copiar", "Renomenar", "Dobrir", "Pegar", "Anullar". This mirrors French/Catalan/Spanish UI style, which use the
  infinitive for buttons (unlike German/English imperative). Recommendation: infinitive for buttons and menu items.
  Confidence: high (consistent across both catalogs).

## Decision points

- **Spelling norm: classical vs Mistralian.** Occitan has two competing orthographies: the classical norm (norma
  classica, the academic and free-software standard) and the Mistralian norm (norma mistralenca, used by some Provençal
  writers). GNOME and Xfce both use the classical norm. Recommendation: classical norm throughout. Confidence: high.
- **Regional variant (dialect).** Occitan spans several dialects (Lengadocian, Provençal, Gascon, Auvernhat, Lemosin,
  Vivaroalpenc). Lengadocian (Languedocien) is treated as the reference koine for the classical norm and is what the
  free-software catalogs target. Recommendation: target Lengadocian under the classical norm. Confidence: high.
- **Formality: informal `tu`.** Covered above. Recommendation: `tu`. Confidence: medium.
- **Buttons: infinitive, not imperative.** Covered above. Confidence: high.
- **Low-resource caveat.** No macOS or Microsoft reference (Apple and Microsoft don't ship Occitan). All evidence is
  Tier 3 (GNOME/Xfce). Terms are well-attested there but unconfirmed by a vendor OS. Recommendation: trust the
  free-software catalogs (`high` where both agree), human-review the rest. Confidence: high (about the gap).
- **Letters and special characters.** Occitan uses `à è ò ó á í ú ç` and the digraph `lh`/`nh`; the interpunct `·`
  appears in some spellings. Never strip diacritics. Confidence: high.
- **Inclusive/gendered language.** Gendered grammar (masculine/feminine nouns and adjectives); generic UI copy avoids
  the issue by addressing the user with `tu` and using infinitives. No special handling beyond avoiding gendered role
  nouns where neutral phrasing exists. Confidence: medium.

## Terminology and glossary

Confirmed against GNOME Nautilus and Xfce Thunar Occitan (Tier 3). Extend as strings come up.

| English term | Occitan     | Notes                    |
| ------------ | ----------- | ------------------------ |
| file         | fichièr     | GNOME                    |
| folder       | dorsièr     | GNOME                    |
| copy         | copiar      | infinitive               |
| move         | desplaçar   |                          |
| delete       | suprimir    |                          |
| trash        | escobilhièr | GNOME; the location noun |
| rename       | renomenar   | infinitive               |
| paste        | pegar       | GNOME                    |
| cut          | talhar      |                          |
| open         | dobrir      | GNOME                    |
| cancel       | anullar     | GNOME                    |
| tab          | onglet      | UI tab, not the key      |
| settings     | paramètres  |                          |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `oc`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('oc').resolvedOptions().pluralCategories`). Note Occitan uses `one` for 0 and 1 and `other` for 2+
(the GNOME/Xfce catalogs encode `plural=(n > 1)`). Every plural message needs both branches; write each as a full
natural phrase since noun gender interacts with the count. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Spelling: classical norm (norma classica), not Mistralian.
- Dialect: Lengadocian reference koine.
- Buttons: infinitive (Romance convention), not imperative.
- Letters: keep all diacritics (`à è ò ó ç`, `lh`/`nh`); never strip.
- Numbers: comma decimal mark, space thousands separator; `Intl` handles formatting at runtime.
- All evidence is Tier 3 (free-software catalogs); no vendor OS ships Occitan.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/oc/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
