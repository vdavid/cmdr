# Luxembourgish (lb) translation style guide

Working notes for translating Cmdr into Luxembourgish. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Luxembourgish.

The reference pile holds only Microsoft sources for `lb-LU` (terminology + style guide); no macOS, GNOME, or Xfce data
exists for Luxembourgish, consistent with no one shipping a full OS/desktop in lb. We write to base tag `lb`.

## Voice and tone

Friendly, concise, active, calm, but rendered through the formal `Dir` register (see Formality), which is the platform
norm for lb software and outweighs importing English-style informality. Error messages stay calm and actionable.

## Formality

Formal `Dir` (+ possessives `Ären` / `Äert` / `Är`), paralleling German `Sie`. Microsoft's Luxembourgish style guide
uses `Dir` throughout its examples ("Wëllt Dir virufueren?", "Gitt Ärem PC en Numm", "Dir wielt d'Bild …"). Buttons and
menu commands use the bare infinitive, capitalized as a label (Kopéieren, Läschen, Ofbriechen), mirroring German
infinitive button labels; the in-prose Dir-imperative form ("Gitt op …") is for sentences, not standalone buttons.

A native reviewer could argue for `du` to sound warmer; record that as a confirm point if lb ships.

## Decision points

Ship lb at all, or fall back to de/fr? (David-only strategic call):
- Luxembourg is trilingual: ~98% French, ~78% German, ~77% Luxembourgish, ~80% English. French dominates written/admin
  use; Luxembourgish is mainly spoken/home use.
- Major-vendor reality: Apple does NOT localize macOS into lb (only optional spellcheck; Luxembourg Mac users get French
  or German UI). Google, Spotify, Netflix: effectively no lb UI. Microsoft is the outlier (terminology + style guide),
  but even it ships few products fully in lb.
- Implication: a Luxembourg user almost certainly already runs their Mac in French or German and would likely pick fr/de
  for Cmdr too. Shipping lb is a goodwill/identity gesture, not a coverage need: de + fr cover the market.
- Recommendation: treat lb as low priority. If lb does ship, gate it behind native review (orthography is unstable, see
  below). Confidence: tentative, leans defer. Flag for David.

Orthography instability:
- Luxembourgish borrows heavily from German but has its own ZLS/Akademie-governed orthography, reformed as recently as
  2019/2020. Spelling is genuinely unstable.
- Recommendation: if shipping, fix the target orthography (current ZLS rules) and require a native reviewer.
  Confidence: high that review is needed.

French nouns + Germanic verbs (don't auto-derive from German):
- Key file-manager nouns are French loanwords, not German coinages: file → `Fichier` (pl. `Fichieren`), NOT "Datei";
  folder → `Dossier`, NOT "Ordner". Verbs lean Germanic (läschen, kopéieren, späicheren). lb is not "German with ë".
- Recommendation: follow Microsoft terminology (glossary below); don't machine-derive lb from de. Confidence: high for
  the listed terms.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Source: Microsoft terminology (`LUXEMBOURGISH.tbx`).

- file → Fichier (pl. Fichieren) · MS · high
- folder → Dossier · MS · high
- copy → kopéieren · MS · high
- move → verréckelen · MS · high
- open → opmaachen · MS · high
- delete → läschen (verb) / Läschen (key/command) · MS · high
- save → späicheren · MS · high
- send → schécken · MS · high
- close → zoumaachen · MS · high
- paste → apechen · MS · high
- cancel → ofbriechen (the dialog Cancel sense) · MS · high, the DB's "deselektionéieren" is deselect, not Cancel
- settings → Parameteren · MS (also Astellungen in some contexts; Parameteren is MS-preferred) · high
- search → Sich · MS · high
- trash / recycle bin → Pabeierkuerf · MS · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one` (n = 1) and `other`, verified against the Unicode CLDR chart (v48). There is NO `two` category
for lb cardinals. Same arity as English/German. The `desktop-i18n-plural` check requires every plural message to cover
the categories this language needs.

## Notes and decisions

- Diacritics: ë, é, è, ä, ö, ü. Don't strip them.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- The big one: ship lb at all, or fall back to de/fr? (lb is a goodwill-only locale; de + fr cover the market.)
- If shipping: confirm formal `Dir` over `du`, and lock the target orthography (current ZLS rules) with a native
  reviewer.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in [`../guides/i18n-translation.md`](../guides/i18n-translation.md)
and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
