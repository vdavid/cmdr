# Malagasy (mg) translation style guide

Working notes for translating Cmdr into Malagasy. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Malagasy.

Sources: a Microsoft Malagasy style guide PDF (filed under `mg-MG/`, the lead authority) and a surprisingly complete but
old GNOME Nautilus catalog (`mg/gnome-nautilus/nautilus.po`, ~95% translated, 2006-2007). No macOS (Tier 1). We write to
base tag `mg`. Native review is essential before ship (no installed-OS reference exists).

## Voice and tone

Friendly, concise, active, calm, but note Microsoft's Malagasy register is formal (see Formality), which is more formal
than Cmdr's usual casual English voice. Aim for one verb per sentence (MS guidance, also helps screen readers). Error
messages stay calm and actionable.

## Formality

Formal register, address pronoun `Ianao` (Microsoft style guide). Avoid the generic personal pronoun `izy`, rewrite to
plural or use articles (`ilay taratasy`) instead of a possessive. Imperatives are morphologically marked with `-y`/`-o`/
`-ao` suffixes for buttons/commands (Sokafy "open", Ajanony "cancel", Fafao "delete"), distinct from the gerund/`Man-`
forms used in descriptions. Follow English casing for UI menu items, titles, and headings.

David call: keep MS's formal register, or soften it toward Cmdr's friendlier voice? Malagasy has limited casual-software
precedent to model a softer voice on. Flagged below.

## Decision points

Dialect target, Standard (Merina-based) Official Malagasy:

- Options: Standard/Official (Merina, central highlands) vs accommodating coastal dialects (Betsimisaraka, Sakalava…).
  The MS style guide and both reference sources are Merina-based.
- Recommendation: target Standard/Official Malagasy (Merina); don't attempt dialect coverage. Confidence: high.

French fallback strategy (vendor-sanctioned):

- French is co-official in Madagascar and dominates computing/business. The MS style guide explicitly names French as
  Madagascar's fallback for untranslatable terms (English acronym + Malagasy or French equivalent in brackets).
- Recommendation: allow French (or kept-English) for technical terms with no settled Malagasy equivalent, in brackets on
  first use as MS does; don't force-coin neologisms. Confidence: high.

VOS word order + fragment-key assembly (real risk):

- Malagasy is VOS (verb-object-subject), rare worldwide. Combined with morphologically-marked imperatives (the `-y`/`-o`
  suffixes), a button verb is not reusable inside a sentence, and the MS guide reinforces "one verb per sentence."
- Recommendation: never build sentences by concatenating translatable fragments; give translators whole strings with
  interpolation slots, and keep imperative command labels (Sokafy, Ajanony) as separate keys from descriptive text.
  Confidence: high.

Near-absence of major-vendor localization:

- Apple does NOT ship Malagasy macOS UI; Windows/Android Malagasy UI is effectively absent; Google has only partial
  web-UI/Translate coverage. The only real authority is the Microsoft style guide + the aging GNOME catalog, no
  installed-OS (Tier 1) reference.
- Recommendation: treat the MS style guide as lead authority, GNOME .po as cross-check, and mark Malagasy a "needs
  native review before ship" language (native review is essential, not optional). Confidence: high.

## Terminology and glossary

From the Nautilus catalog (cross-checked against MS). Imperatives use the `-y`/`-o`/`-ao` button form. Format:
`English → chosen · source · confidence`.

- file → Rakitra · GNOME · tentative
- folder → laha-tahiry · GNOME · tentative
- copy → Mandika (verb) / Adikao (imperative) · GNOME · tentative
- move (here) → Afindrao (eto) · GNOME · tentative
- paste → Apetao / Mametaka · GNOME · tentative
- cancel → Ajanony · GNOME · tentative
- open → Sokafy (imperative) / Manokatra · GNOME · tentative
- delete → Fafao (imperative) / Mamafa · GNOME · tentative
- rename → Ovay anarana · GNOME · tentative
- trash → daba · GNOME · tentative

Recommended Malagasy dictionaries for term work (per MS guide): tenymalagasy.org, malagasyword.org.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other`, where `one` covers BOTH 0 and 1 (other = 2+). So "0 items" takes the same form as "1
item". Confirmed by the Nautilus header `nplurals=2; plural=n>1`, which matches CLDR. The `desktop-i18n-plural` check
requires every plural message to cover the categories this language needs.

## Notes and decisions

- Diacritics: ô, à (limited). Don't strip them.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- Register: keep Microsoft's formal `Ianao` register, or soften toward Cmdr's friendlier voice?
- The reference catalog is GNOME-only for terms (no macOS, no MS terminology TBX); confirm key terms with a native
  reviewer before shipping.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in
[`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/mg/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
