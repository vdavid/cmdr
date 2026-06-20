# Lithuanian (lt) translation style guide

Working notes for translating Cmdr into Lithuanian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Lithuanian.

## Voice and tone

Friendly, concise, active, calm. Lithuanian UI convention already drops the second-person pronoun and addresses the user
through the verb, which suits Cmdr's direct voice. Error messages stay calm and actionable; state the problem and a next
step, and don't use a bare "klaida" label.

## Formality

Formal register (`jūs`), pronoun usually omitted. Microsoft's Lithuanian style guide is explicit: address the user in
the second-person plural, normally dropping the pronoun, and lowercase `jūs` if it must appear. Informal `tu` reads as
oddly intimate for a tool. Buttons and menu commands use the infinitive (Kopijuoti, Atverti, Ieškoti), matching every
file-manager source (GNOME, Xfce); the imperative is fine inside a direct confirmation prompt.

Open sub-question: `jūs` vs capitalized `Jūs`. Microsoft uses lowercase; traditional Lithuanian politeness capitalizes
it. Flagged below.

## Decision points

No macOS anchor (priority signal):

- Apple does NOT ship a Lithuanian macOS UI, so there's no Tier-1 Finder reference; Lithuanian Mac users run English or
  another language. Authority rests on Microsoft (Tier 2: full terminology DB + style guide) and GNOME/Xfce (Tier 3),
  both well-sourced. Recommendation: lean on Microsoft for terms, GNOME/Xfce for file-manager parity, and budget native
  review before shipping (no native Finder to check against). Confidence: high.

Case system vs placeholder insertion (the #1 structural risk):

- Lithuanian nouns inflect across 7 cases x 2 numbers x several declensions; a `{name}`/`{count}` dropped into a
  sentence forces agreement the template can't satisfy. "Perkelti {name} į {folder}" forces both placeholders into the
  wrong case.
- How the majors cope: they avoid it, not solve it. Keep filename placeholders quoted and in nominative, and restructure
  the sentence so the variable sits where nominative is grammatical (lead with it, or use a colon/label form like
  `„{name}" – perkelta`). No runtime case engine exists; this is authoring discipline.
- Recommendation: design lt message templates so placeholders never need to inflect, and treat every string with a
  noun/filename/count variable as a native-review item. This is a message-architecture rule, not just translation.
  Confidence: high.

CLDR plurals (four categories, easy to under-cover):

- lt categories: `one`, `few`, `many`, `other` (verified, CLDR v48).
  - one: n%10=1 and n%100 not in 11..19 (1, 21, 31)
  - few: n%10=2..9 and n%100 not in 11..19 (2-9, 22-29)
  - many: fractions only (1.5, 10.1), integers never hit this
  - other: everything else (0, 10-20, 11-19, 100)
- "21 failai" vs "2 failai" vs "11 failų" are three different forms. Do NOT collapse to English one/other. The `many`
  branch is only needed for fractional counts (file sizes like "1,5 GB"); for integer item counts still cover
  one/few/other. The gettext catalogs declare 3 forms (older rule omits fractional `many`); follow CLDR's 4-category
  model since Cmdr's intl layer is CLDR-based. Confidence: high.

Terminology splits (Microsoft vs GNOME; David to settle):

- Cmdr's friendly voice tilts toward the everyday GNOME words over Microsoft's more technical picks, but pick one and
  don't mix:
  - cancel: Atšaukti (MS) vs Atsisakyti (GNOME/Xfce). Recommend Atšaukti (standard modern UI button).
  - delete: Ištrinti (GNOME, everyday) vs Naikinti (MS house style). Recommend Ištrinti. Note ištrinti (delete) vs
    pašalinti (remove) are distinct.
  - settings: Nuostatos (GNOME, friendlier) vs Parametrai (MS). Recommend Nuostatos.
  - open: Atverti (GNOME) vs Atidaryti (general MS). Tentative.
- Confidence: tentative (needs David / native pick).

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Sources agree unless noted.

- file → failas · MS, GNOME, Xfce · high
- folder → aplankas · MS, GNOME · high
- copy → kopijuoti · MS, GNOME · high
- move → perkelti · GNOME (inflected forms in catalog) · high
- send → siųsti · MS, GNOME · high
- search → ieškoti (action) / paieška (noun) · GNOME, MS · high
- rename → pervadinti · GNOME · high
- properties → savybės · GNOME, Xfce · high
- version → versija · MS · high
- trash → šiukšlinė · GNOME, Xfce, MS · high
- cancel → Atšaukti · MS (GNOME: Atsisakyti) · tentative
- delete → ištrinti · GNOME (MS: naikinti) · tentative
- settings → nuostatos · GNOME (MS: parametrai) · tentative
- open → atverti · GNOME (MS: atidaryti) · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `few`, `many`, `other` (see Decision points for the exact rules). Cover one/few/other for any
counted noun; add `many` only where fractional counts appear. The `desktop-i18n-plural` check requires every plural
message to cover the categories this language needs.

## Notes and decisions

- Sentence case: Lithuanian capitalizes only the first word and proper nouns, fitting the app's sentence-case rule.
- Diacritics: ą č ę ė į š ų ū ž. Don't strip them. Overflow-check against the pseudolocale (`en-XA`).
- Numbers and dates come from the formatter layer (comma decimal). Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- Whether to ship Lithuanian without a macOS anchor, and the native-review budget (higher need than de/sv/fr).
- `jūs` vs `Jūs` capitalization (Microsoft lowercase vs traditional politeness form).
- The term splits: cancel (Atšaukti/Atsisakyti), delete (Ištrinti/Naikinti), settings (Nuostatos/Parametrai), open
  (Atverti/Atidaryti).

## ICU mechanics

Catalog-level, language-agnostic, easy to miss: double every apostrophe in a value (`'` → `''`; ICU swallows text after
a lone `'`), and keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
[`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/lt/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
