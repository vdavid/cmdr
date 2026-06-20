# Basque (eu) translation style guide

Working notes for translating Cmdr into Basque (Euskara). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Basque.

`eu` is the language base (Euskara Batua, the standardized literary Basque). The reference pile has four sources
(Microsoft terminology, Microsoft style guide, GNOME nautilus, Xfce thunar) but no macOS (Apple does not ship a Basque
macOS UI), so Microsoft and GNOME/Xfce carry the term evidence here.

## Voice and tone

Friendly, concise, active, calm, never alarmist. The Microsoft Basque style guide explicitly steers toward the same
register Cmdr wants: "less formal, more grounded", "to the point", "everyday words", and warns against "long, formal, or
obscure constructions" in favor of "simpler, more direct syntax" (verified in `eu/microsoft-style-guides/`, 2026-06-20).
Lean into that. Error messages state the problem and a next step; never use the bare labels "error"/"failed".

## Formality

**Use the standard `zu` register (zuka), recommended.** Basque has two address registers: the standard polite-neutral
`zu` form (zuka), and the intimate `hi` form (hika). Software universally uses `zu`:

- Microsoft Basque addresses the user with the second person in the standard `zu` form throughout (the style guide
  discusses second-person address for asking the user to continue or take action; it never invokes hika). Verified in
  `eu/microsoft-style-guides/`, 2026-06-20.
- GNOME/Xfce Basque catalogs use `zu`-register imperatives.
- `hi` (hika) is too intimate for a product UI and carries gendered allocutive verb forms (it agrees with the
  addressee's gender), which a UI cannot resolve. Never use hika.
- Recommendation: `zu` register throughout. Confidence: high. Note Basque is not a T/V language in the
  Romance/Germanic sense; `zu` is the single neutral choice, so there's no formal-vs-informal call to make beyond
  "don't use hika".

**Imperatives for UI actions**: use the standard imperative consistent with `zu`, following the GNOME/Microsoft Basque
conventions for file-manager actions (often the verb root + "-tu/-i" forms, e.g. "Kopiatu", "Ezabatu").

## Decision points

Formality is settled above (`zu`/zuka). The genuinely tricky parts of Basque are grammatical, not script or variant.

- **Script: Latin only. No decision.** Basque is written in the Latin script with standard orthography (Euskaltzaindia's
  norms). No Cyrillic/other-script variant exists. Confidence: confirmed.
- **Regional variant: target Euskara Batua (the standard), no region split needed.** Basque has dialects (Bizkaian,
  Gipuzkoan, etc.), but software localizes to Euskara Batua, the unified standard literary form. Microsoft and GNOME
  both target Batua. There is no `eu-ES` vs `eu-FR` product split in practice (the language spans Spain and France, but
  the written standard is shared). Recommendation: write Batua. Confidence: high.
- **Agglutination and case suffixes are the real translation difficulty, not gender.** Basque has no grammatical gender,
  so the gender-agreement traps of Romance languages don't apply. Instead, Basque is heavily agglutinative: the article
  and case ending attach as suffixes to the noun, and the suffix form depends on whether the stem ends in a vowel or
  consonant. This bites hardest with `{placeholder}` inserts: a sentence like "Move {name} to trash" can't safely
  bolt a fixed case suffix onto `{name}`, because the suffix that's grammatical depends on the (unknown, runtime) final
  sound of the inserted value. Structure such sentences to avoid suffixing directly onto an uncontrolled placeholder
  (e.g. quote the name and use a postposition or a neutral frame). This is the single biggest blind-translation risk for
  Basque. Confidence: high; flag for translator awareness, no David call needed.
- **Ergative alignment / word order.** Basque is ergative-absolutive and default SOV. Phrasing reads naturally only when
  the translator respects this, especially in *Join fragment keys where the assembly order is set in the join key.
  Confidence: high; a translator concern, not a David call.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order (no macOS for Basque): Microsoft (Tier 2) →
GNOME/Xfce (Tier 3).

- copy → Kopiatu · MS terminology / GNOME ("Kopiatu") · high (GNOME's bare "Kopia" is the noun; "Kopiatu" is the action)
- trash → Zakarrontzia · GNOME nautilus ("Zakarrontzia") · high
- move to trash → Bota zakarrontzira · GNOME nautilus ("Bota zakarrontzira") · high
- delete / remove → Ezabatu · MS terminology / GNOME ("Ezabatu") · high
- cancel → Utzi / Bertan behera utzi · MS / GNOME · high
- search → Bilatu · GNOME ("Bilatu") · high
- file → fitxategi · MS / GNOME ("fitxategi") · high
- folder → karpeta · MS / GNOME ("karpeta") · high
- settings → Ezarpenak · MS / GNOME ("Ezarpenak") · high
- version → bertsio · MS terminology · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens and `{email}`. Enforced by `desktop-i18n-dont-translate` (list in
`apps/desktop/scripts/i18n-catalog-lib.js`). Note that Basque case suffixes attach to brand words too in running text
("Cmdr-ek", "GitHub-en"); keep the brand stem verbatim and let the suffix follow with a hyphen as Basque convention
allows, but never alter the brand token itself.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('eu')`, 2026-06-20). Two branches. Basque marks
plurality on the article/case suffix, not a separate plural word, so the count agreement shows up in the suffix; write
both branches so the counted noun's suffix is correct for one vs many.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Length: Basque words can be long** because case suffixes stack onto the stem; agglutinated forms run longer than the
  English. Overflow-check tight buttons against the pseudolocale (`en-XA`).
- **Quotation marks.** Basque commonly uses `«…»` (guillemets) in print, but UI catalogs vary; match the surrounding
  source convention and keep it consistent. No counted/quoted strings force a call in the crash set.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- No David-only calls. Formality (`zu`), script (Latin), and variant (Batua) are all settled by the sources. The
  agglutination/placeholder-suffix issue is a translator-craft concern, not a project-owner decision.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/eu/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
