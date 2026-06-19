# Konkani (kok) translation style guide

Working notes for translating Cmdr into Konkani (कोंकणी). Read [`README.md`](README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes
carry into Konkani.

Sourced: the pile has MS terminology and MS style guide (`_ignored/i18n/kok/`); no macOS, no GNOME, no Xfce. MS is the
only authority here. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **Script: target Devanagari (`kok` / `kok-Deva`), recommended, but Konkani is genuinely multi-script (the key flag,
  high).** Konkani is unusual: it's officially written in multiple scripts across communities: Devanagari (the
  official script of Goa and the standard for most digital localization), Roman/Latin (used by Goan Catholic
  communities), Kannada (in coastal Karnataka), and historically Malayalam and Perso-Arabic. MS Konkani localizes in
  **Devanagari** and cites the Goa Konkani Academi orthography rules (verified 2026-06-20). Recommendation: **target
  Devanagari** as the single shipped script; treat Roman-Konkani as a possible future `kok-Latn` variant only if users
  ask. Flagging because the multi-script reality is a real product decision, not a detail.
- **Low-resource: MS is the only source.** No file-manager catalog (GNOME/Xfce) for Konkani, so core file-manager terms
  lean on MS terminology plus native review; several will be `tentative`.

## Voice and tone

Friendly, concise, active, calm, never alarmist. MS Konkani targets the conversational, everyday register over formal
technical language (verified 2026-06-20), matching Cmdr's English voice. Error messages stay calm and actionable: name
the problem and the next step, and avoid a bare "चूक" (error) status label the way English avoids "error"/"failed".

## Formality

- **Polite second person, addressing the user directly.** Konkani (like neighboring Marathi) distinguishes a polite
  second-person form (`तुमी`) from a familiar one (`तूं`). Software uses the polite form. Recommended default: **polite
  `तुमी`-register throughout.** Confidence: medium-high; a native reviewer confirms the exact form, since Konkani
  pronoun usage varies by community/region.
- **Action labels (buttons, menu items): the established imperative form.** No GNOME catalog to anchor verb labels;
  follow MS terminology and the Goa Konkani Academi conventions, keeping labels short. Use full polite-form verbs in
  sentences to the user. Confidence: medium pending a term pass.

## Decision points

- **Script: Devanagari, with a real multi-script backdrop (the key decision, high).** Ship Devanagari (`kok`), the
  official Goa script and MS's localization script (verified 2026-06-20). Devanagari is an abugida and is unicameral
  (no case), so title-case-vs-sentence-case collapses to one letterform. Roman-Konkani and Kannada-Konkani exist but
  aren't the localization target unless a `kok-Latn`/`kok-Knda` variant is later requested. Confidence: high that
  Devanagari is the right default.
- **Regional variant: one shipped, `kok` (`kok-IN`).** Konkani is official in Goa; treat it as a single Devanagari
  target. The script variants above are the real axis of variation, not region. Confidence: high.
- **Gender / inclusive language (medium-high problem).** Konkani has grammatical gender (masculine/feminine/neuter) and
  verb agreement that can expose the subject's gender. A sentence addressed to the user in past tense may force a
  gender. Where it would, **rewrite impersonally** or use a neuter/agentless construction. A native reviewer handles
  the agreement; this is a more live concern than in the genderless languages in this batch. Confidence: medium-high.
- **Capitalization: not applicable.** Devanagari has no case. Don't capitalize labels. Confidence: confirmed.
- **Agglutination/postpositions affect placeholder grammar (high).** Like other Indian languages, Konkani uses
  postpositions and case-marked nouns. A `{path}`/`{name}` placeholder before a postposition can't reliably attach to
  runtime text. Structure sentences so a placeholder lands where the grammar doesn't depend on its ending. A native
  reviewer handles the edges. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/kok/` (MS terminology, MS
style guide) on 2026-06-20; no macOS or GNOME for Konkani, so MS terminology is the only authority and core
file-manager terms need native review. Sources decide the term; Cmdr writes its own value (MS copyrighted, never copied
verbatim).

To settle from MS terminology (`kok/microsoft-terminology/`) with a native check (expect most to start `tentative`):

- **file, folder** · MS terminology; cross-check with Marathi conventions where Konkani is thin. `tentative`.
- **trash / move to trash** · MS terminology; confirm the recycle-bin sense. `tentative`.
- **copy, open, cancel, delete, rename, eject** · MS terminology for each action verb. `tentative`.
- **volume / pane / tab** · MS terminology if present, else native review. `tentative`.

Mark a term `high` only once a native reviewer or a second source confirms it; single-source MS is `tentative` for the
file-manager domain.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand names stay in Latin script inside Devanagari text.

## Plurals

CLDR categories for `kok`: `one`, `other` (verified with `new Intl.PluralRules('kok')`, 2026-06-20). Write both.

- **one**: integer 1. "1 फायल".
- **other**: everything else, including 0 and counts ≥ 2. Konkani marks plural and gender on nouns, so the `other`
  branch typically pluralizes the counted noun, with gender agreement the native reviewer settles. The
  `desktop-i18n-plural` check requires both.

## Notes and decisions

- **Quotation marks:** Devanagari-Konkani UI commonly uses English-style `"…"`; a native reviewer settles house style.
- **Numbers and dates come from the formatter layer.** Devanagari has its own digit glyphs but Arabic digits are
  standard in modern UI; `formatNumber()`/`formatBytes()` follow the locale. Never hardcode separators in a string.
- **Length and height.** Devanagari can render taller (matras above/below the baseline); overflow-check both width and
  line-height against the pseudolocale (`en-XA`) and a Devanagari font.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
