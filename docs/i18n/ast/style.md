# Asturian (ast) translation style guide

Working notes for translating Cmdr into Asturian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Asturian.

**Sparse pile, no macOS, no Microsoft.** Apple ships no Asturian macOS UI and Microsoft has no Asturian terminology or
style guide. The pile has GNOME Nautilus + Xfce Thunar for `ast` (`_ignored/i18n/ast/`). Asturian is a Romance language
of Asturias, Spain (~100k-600k speakers depending on the count), with an active language academy (ALLA) and a more
developed UI-localization tradition than Aragonese. Terms lean on GNOME + Xfce; native review still required. Evidence
verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone.

- **Scope / priority: low-to-medium (high confidence it's low priority).** No Apple/Microsoft reference, but Asturian
  has a real FOSS localization community (GNOME, Xfce, LibreOffice ship in Asturian) and a standardizing academy, so the
  quality bar is reachable. Recommend translating only if there's specific demand, as a community-reviewed effort. Flag
  for David.
- **Address form: informal singular ("tu") recommended (tentative).** Asturian, like the neighboring Spanish UI norm,
  addresses the user informally with "tu". GNOME Asturian uses infinitive labels (no direct address captured), so this
  is inferred from the Spanish/Romance convention. Recommended default: informal singular. Confidence: tentative -
  native call.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. With GNOME + Xfce as references (and the
closely-related Spanish, `es/style.md`, as a register neighbor), keep it plain and idiomatic Asturian. Error messages
stay calm and actionable: phrase the problem and the next step. Native review is essential.

## Formality

- **Likely informal singular "tu"**, mirroring the Spanish/Romance UI convention, but unconfirmed for Asturian
  specifically. Native reviewer (ALLA-aligned) to settle.
- **Action labels (buttons, menu items): infinitive.** The Romance UI norm and what GNOME Asturian shows: "Encaboxar"
  (Cancel), "Guetar" (Search) (GNOME Nautilus, verified 2026-06-20). Labels use the infinitive, like Spanish/Catalan/
  Galician. Confidence: high for labels (GNOME-backed); sentence-level register is tentative.

## Decision points

- **Script: Latin, no decision.** Asturian uses the Latin alphabet with ALLA-standardized orthography. No script choice.
  Confidence: high.
- **Regional variant: one, `ast`.** Asturian has dialectal variation (central/western/eastern), but the ALLA standard
  ("asturianu estándar", central-based) is the single UI target. Don't build a variant matrix. (Mirandese and Leonese
  are related but separate.) Confidence: high.
- **Gender / inclusive language (a Romance concern, tentative).** Asturian has grammatical gender (masc/fem, plus a
  distinctive Asturian neuter-of-matter on adjectives, but that's for uncountable mass nouns, not person reference), so
  adjectives/participles referring to the user agree. Mirror the Spanish strategy: prefer impersonal/nominal phrasing to
  avoid a gender guess ("Copia fecha" rather than a gendered "Copiasti…"). Confidence: tentative.
- **Capitalization: sentence case everywhere (high).** Romance convention and GNOME/Xfce usage. Matches Cmdr's
  sentence-case rule. Confidence: high.
- **Closeness to Spanish (the practical reality).** Nearly all Asturian speakers are fluent in Spanish; a clumsy
  Asturian reads worse than good Spanish to them. The bar is "natively idiomatic or don't ship it". Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/ast/` (GNOME Nautilus, Xfce
Thunar; NO macOS, NO Microsoft) on 2026-06-20. Sources decide the term; Cmdr writes its own value (GNOME/Xfce GPL, never
copied verbatim). Terms are `high`-via-GNOME where captured, else `tentative` pending native review.

- **trash: `papelera`** · GNOME ("Papelera"). `high` (GNOME).
- **cancel: `encaboxar`** · GNOME ("Encaboxar"). Distinctly Asturian (vs Spanish "cancelar"). `high` (GNOME).
- **search: `guetar`** · GNOME ("Guetar", to look for). `high` (GNOME).
- **sidebar: `barra llateral`** · GNOME ("Barra llateral"). Note the Asturian ll-. `high` (GNOME).
- **network: `rede`** · GNOME ("Rede"). `high` (GNOME).
- **folder, file, open, rename, eject, volume, pane, tab, bookmark** · not captured; defer to a native reviewer (ALLA /
  Softastur community resources). `tentative`.

Add terms as they come up; the Softastur localization community is the practical authority for Asturian UI terms.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories for `ast`: `one`, `other` (verified with `new Intl.PluralRules('ast')`). Only two forms, the standard
Romance singular/plural. The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets, the Spanish/Romance convention). Avoid straight ASCII `"`.
- **Numbers and dates come from the formatter layer.** Asturian (Spain) uses a comma decimal and period/space thousands
  separator; `formatNumber()`/`formatBytes()` produce locale-correct output. Never hardcode separators.
- **Length.** Romance text runs longer than English; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Asturian elides the article before a vowel with an apostrophe ("l'usuariu",
  "d'esti") - those real apostrophes must be DOUBLED in ICU values, an Asturian-specific trap. Full rules: the
  agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/ast/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
