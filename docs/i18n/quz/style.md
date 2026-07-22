# Quechua (Cusco, quz) translation style guide

Working notes for translating Cmdr into Cusco Quechua. Read `../README.md` for how this fits the translation process.

LOW priority but better-precedented than most low-tier languages. `quz` is Cusco Quechua (Southern Quechua, Peru, ~1.5M
speakers). The pile has Microsoft terminology AND a Microsoft style guide for `quz`; no macOS (Apple ships no Quechua
UI), no GNOME/Xfce. Microsoft localized Windows/Office into Quechua, so a real software register exists to anchor on.

## Voice and tone

Friendly, concise, active, never alarmist. Follow Microsoft's Quechua style guide register (the only authoritative
software-tone reference). Keep it plain and clear; native review still required.

## Formality

Latin script (the standardized Southern Quechua trivocalic orthography, a/i/u). Quechua has respectful registers but the
software convention from Microsoft is plain/neutral. Use imperative for actions. Confidence: tentative-to-high
(Microsoft style guide gives real grounding).

## Decision points

### Which Quechua variant (the defining choice)

"Quechua" is a family, not one language, and variants are often NOT mutually intelligible (Cusco Quechua vs Ecuadorian
Kichwa are unrecognizable to each other). The tag `quz` pins this to **Cusco/Southern Quechua specifically**.

- Microsoft targeted Southern/Cusco Quechua (`quz`) for its Windows/Office localization; that's the most-resourced
  variant and the one in the pile.
- Recommendation: target `quz` (Cusco Quechua) exactly as tagged; do NOT blend with Bolivian (`quh`), Ayacucho (`quy`),
  or Ecuadorian Kichwa (`qu`/`quw`). Confidence: high.
- Flag: this is only worth doing if the Peruvian/Cusco audience is the target. Confirm scope.

### Orthography: trivocalic standard vs pentavocalic

Cusco Quechua spelling is contested: the official Peruvian standard uses three vowels (a, i, u), but some writers use
five (adding e, o). Microsoft's style guide picks one standard.

- Recommendation: follow Microsoft's Quechua style guide orthography (trivocalic, the official Peruvian Ministry of
  Education standard) for consistency with the only software precedent. Confidence: high (defer to the style guide).

### Spanish loanwords for tech terms

Quechua has limited native vocabulary for computing concepts; localizations either coin Quechua neologisms or borrow
Spanish. Microsoft's approach mixes coined terms with Spanish loans. Recommendation: follow Microsoft's choices term by
term rather than inventing neologisms; for "file/folder/copy" check the style guide and terminology first. Confidence:
high on the approach, tentative on specific terms pending the style-guide mining.

### Numerals

Western digits; `Intl` handles formatting. Confidence: high.

## Terminology and glossary

Defer; Microsoft quz terminology + style guide are the sources. Mine them before coining anything. Native review still
needed for confidence above tentative.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `quz`: `one`, `other`. Two forms. (Quechua marks plural with the suffix -kuna, but CLDR selection
needs only one/other.)

## Decisions to confirm with David

- Is Quechua in scope, and specifically Cusco/Southern `quz` for a Peruvian audience? Low priority overall, but
  Microsoft precedent makes it more feasible than other low-tier languages if there's demand.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/quz/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
