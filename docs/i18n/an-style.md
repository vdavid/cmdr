# Aragonese (an) translation style guide

Working notes for translating Cmdr into Aragonese. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Aragonese.

**Very sparse pile, no macOS, no Microsoft.** Apple ships no Aragonese macOS UI and Microsoft has no Aragonese
terminology or style guide. The pile has ONLY GNOME Nautilus for `an` (`_ignored/i18n/an/`). Aragonese is a minority
Romance language of Aragon, Spain (a few thousand active speakers), closely related to Spanish and Catalan. The sole
reference is the GNOME catalog; native review is mandatory. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone.

- **Scope / priority: low (high confidence it's low).** With only a GNOME catalog and a tiny speaker base, Aragonese is
  a low-priority, community-goodwill locale. Recommend translating only if there's specific demand, and treating it as a
  community-reviewed effort. Flag for David.
- **Address form: informal singular ("tú"-equivalent "tu") recommended (tentative).** Aragonese, like the Spanish UI
  norm it sits next to, can address the user informally. GNOME Aragonese uses infinitive labels (no direct address to
  inspect), so this is inferred from the Spanish/Catalan neighbor convention rather than a hard source. Recommended
  default: informal singular, mirroring the `es` decision. Confidence: tentative - native call.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. With only GNOME as a reference, lean on
the closely-related Spanish (`es-style.md`) conventions for register, adapted to Aragonese vocabulary. Error messages
stay calm and actionable: phrase the problem and the next step. Native review is essential - the agent draft here is
thinner than for well-sourced languages.

## Formality

- **Likely informal singular**, mirroring the Spanish UI convention (see `es-style.md`), but unconfirmed for Aragonese
  specifically. Native reviewer to settle.
- **Action labels (buttons, menu items): infinitive.** The Romance UI norm and what GNOME Aragonese shows: "Ubrir"
  (Open), "Cancelar" (Cancel), "Mirar" (Search) (GNOME Nautilus, verified 2026-06-20). So labels use the infinitive,
  like Spanish/Catalan. Confidence: high for labels (GNOME-backed); the sentence-level register is tentative.

## Decision points

- **Script: Latin, no decision.** Aragonese uses the Latin alphabet (with Spanish-like orthography; the standardized
  "grafía" has some contested points among Aragonese language bodies, but that's an orthographic-standard question for a
  native reviewer, not a script choice). Confidence: high on Latin script.
- **Regional variant: one, `an`.** Aragonese has dialectal variation (the Pyrenean valleys differ), and orthographic
  standardization is itself contested between language academies, but for a UI there's one practical target. Don't build
  a variant matrix. Confidence: high (practical), with the orthographic-standard caveat for a reviewer.
- **Gender / inclusive language (a Romance concern, tentative).** Aragonese has grammatical gender (masc/fem) like
  Spanish, so adjectives/participles referring to the user agree. Mirror the Spanish strategy: prefer impersonal/nominal
  phrasing to avoid a gender guess ("Copia rematada" rather than a gendered "Has copiau…"). Confidence: tentative.
- **Capitalization: sentence case everywhere (high).** Romance convention and GNOME usage. Matches Cmdr's sentence-case
  rule. Confidence: high.
- **Closeness to Spanish (the practical reality).** Many users read Spanish fluently; a clumsy Aragonese reads worse
  than good Spanish to them. The bar for Aragonese is "natively idiomatic or don't ship it". Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/an/` (GNOME Nautilus only; NO
macOS, NO Microsoft) on 2026-06-20. Sources decide the term; Cmdr writes its own value (GNOME GPL, never copied
verbatim). Everything is `tentative` to `high`-via-GNOME only, pending native review.

- **folder: `carpeta`** · GNOME ("Carpeta"). `high` (GNOME).
- **trash: `papelera`** · GNOME ("Papelera"). `high` (GNOME).
- **open: `ubrir`** · GNOME ("Ubrir"). Note the Aragonese form (vs Spanish "abrir"). `high` (GNOME).
- **cancel: `cancelar`** · GNOME ("Cancelar"). `high` (GNOME).
- **search: `mirar`** · GNOME ("Mirar", lit. to look). `high` (GNOME).
- **sidebar: `barra lateral`** · GNOME ("Barra lateral"). `high` (GNOME).
- **file, rename, eject, volume, pane, tab, bookmark** · not captured from GNOME; defer to a native reviewer (Aragonese
  language academy resources). `tentative`.

Add terms as they come up; GNOME is the only source, so native review carries the real authority here.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories for `an`: `one`, `other` (verified with `new Intl.PluralRules('an')`). Only two forms, the standard
Romance singular/plural. The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets, the Spanish/Romance convention). Avoid straight ASCII `"`.
- **Numbers and dates come from the formatter layer.** Aragonese (Spain) uses a comma decimal and period/space thousands
  separator; `formatNumber()`/`formatBytes()` produce locale-correct output. Never hardcode separators.
- **Length.** Romance text runs longer than English; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
