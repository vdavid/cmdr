# Catalan (ca) translation style guide

Working notes for translating Cmdr into Catalan. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Catalan.

`ca` is the base (Central Catalan, the standard the macOS Catalan UI uses). The Valencian variant (`ca-valencia`) is a
real, separately-shippable form covered under Decision points; it carries overrides where Valencian usage diverges, with
`ca` as its fallback.

Catalan is the data-rich language in this batch: the reference pile has all five sources (macOS, Microsoft terminology,
Microsoft style guide, GNOME nautilus, Xfce thunar), so most calls below are evidence-backed rather than guesses.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Catalan UI copy can drift formal and long; resist it. Prefer a verb
over a verbal noun ("Cerca", not "Fes una cerca"). Error messages state the problem and a next step, and never use the
bare labels "error"/"failed" (Cmdr's voice rule is stricter than macOS Catalan, which does say "S'ha produït un error").

## Formality: informal `tu`, settled

**Address the user as `tu`** (informal second person), settled from the sources:

- macOS Catalan is fully informal. Across mined Finder/AppKit strings, second-person address is `tu`: 194 instances of
  `vols` (informal "you want"), plus `pots` ("you can"), `el teu`/`la teva` (informal "your"), and zero `vostè` (the
  formal pronoun). Finder phrases the strings we care about informally: "Selecciona com vols sincronitzar-les",
  "Introdueix el nom del grup que vols afegir" (verified in `ca/macOS/`, grep over Finder + AppKit, 2026-06-19).
- Cmdr is a macOS app with a friendly voice that signs onboarding as David, so `tu` is both the macOS-native choice and
  the right tonal fit. Confidence: high.

## Formality mechanics

- **`tu`**, throughout. Imperatives addressed to the user take the `tu` form ("Selecciona…", "Activa…"), matching macOS
  Finder ("Selecciona…", "Introdueix…").
- **Buttons and menu items: imperative `tu` form.** macOS Catalan labels action buttons with the imperative ("Copia",
  "Cancel·la", "Expulsa", "Mostra'n menys"), not the infinitive (this differs from Spanish, which uses the infinitive
  for buttons). Follow macOS: "Copia", "Cancel·la", "Cerca", "Envia".

## Decision points

Formality is settled above. The big remaining Catalan call is the Valencian variant.

- **Regional variant: target Central Catalan (`ca` base), offer `ca-valencia` only on demand.** Catalan splits into
  Central Catalan (the macOS base, used in Catalonia) and Valencian (`ca-valencia`, used in the Valencian Community,
  with its own official body, the AVL). Both are real shipping targets:
  - Apple ships a single "Català" (Central); it does not ship a separate Valencian macOS.
  - Microsoft ships both "Català" and "Valencià" as distinct localizations (the reference pile has a `ca-ES-valencia` MS
    folder), and the GNOME/Xfce projects carry a separate `ca@valencia` catalog.
  - Differences that surface in a file-manager UI are narrow but real: Valencian prefers the demonstratives "este/esta"
    over Central "aquest/aquesta", differs in some verb morphology (Valencian "ix" vs Central "surt"), and diverges on a
    handful of nouns. None of these block a single Central base from reading acceptably to a Valencian user.
  - Recommendation: write the `ca` base in Central Catalan (matches macOS, the user's OS language), and add a
    `ca-valencia` override file only if a Valencian user asks. Confidence: high. The David-only call: whether Cmdr's
    primary Catalan audience warrants shipping the Valencian variant up front. Flag for David.
- **Gendered grammar: prefer direct `tu`-address and neutral nouns; no inclusive "-i/-e/x" endings in UI.** Catalan
  nouns are gendered ("l'usuari"/"la usuària"). macOS and Microsoft Catalan both avoid gendering the user by using
  direct address ("Selecciona…", "Vols…?") and neutral nouns, and neither ships inclusive-ending experiments in core
  product UI. Recommendation: direct `tu`-address and neutral nouns. Confidence: high.
- **Middle dot (l·l).** Catalan's geminate l uses the interpunct character `l·l` ("cancel·la", "instal·la"). This is
  orthography, not a judgment call, but it's a frequent typo source: use the real `·` (U+00B7), not a period. macOS
  Catalan uses it ("Cancel·la", "instal·lar"). Verified in `ca/macOS/`, 2026-06-19.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order is macOS (Tier 1) → Microsoft (Tier 2) →
GNOME/Xfce (Tier 3).

- copy → Copia · macOS Finder ("Copia") · high
- cancel → Cancel·la · macOS AppKit ("Cancel·la") · high
- eject → Expulsa · macOS Finder ("Expulsa") · high
- trash → paperera · macOS Finder ("paperera", "a la paperera") · high
- Quick Look → Vista ràpida · macOS Finder ("Vista ràpida") · high, but "Quick Look" is a brand token kept verbatim in
  Cmdr (see Brand below); use "Vista ràpida" only for the generic concept, not the feature name
- search → Cerca · macOS / GNOME ("Cerca") · high
- settings → Configuració · macOS System Settings ("Configuració del sistema") · high
- send → Envia · macOS Finder AirDrop ("Envia", "Enviant…") · high
- version → versió · MS terminology · high
- delete / remove → Elimina · macOS Finder ("Elimina", "S'eliminaran… ítems") · high
- file → fitxer · macOS / GNOME ("fitxer") · high
- folder → carpeta · macOS / MS ("carpeta") · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens
and `{email}`. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

## Plurals

CLDR categories: `one`, `many`, `other` (verified with `new Intl.PluralRules('ca')`, 2026-06-20). The `many` category
covers fractional/large-number forms; write all three branches where a string is counted. Catalan nouns carry
grammatical gender; articles and adjectives agree with the counted noun in every branch.

## Notes and decisions

- **Apostrophe and the geminate `l·l`.** Catalan uses the typographic apostrophe in contractions ("l'usuari",
  "d'aquest"). In ICU values, double EVERY apostrophe regardless of which character; if the source uses the curly `'`,
  it still needs doubling per ICU's escape rule. Keep the interpunct `·` in geminate l (see Decision points).
- **Quotation marks.** Catalan traditionally uses `«…»` (guillemets), but macOS Catalan UI strings use curly `"…"`
  (verified in `ca/macOS/Finder/`, 2026-06-19). Match macOS: curly double quotes.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Length: Catalan runs ~10–20% longer than English.** Overflow-check tight buttons against the pseudolocale (`en-XA`).
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Ship the Valencian variant (`ca-valencia`) up front, or only on demand?** The recommendation is Central-only base
  plus a Valencian override file if asked; confirm whether the Valencian audience warrants shipping it from the start.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ca/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
