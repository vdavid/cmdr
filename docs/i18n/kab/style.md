# Kabyle (kab) translation style guide

Working notes for translating Cmdr into Kabyle (Taqbaylit). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Kabyle.

`kab` is the language base. The reference pile has only GNOME Nautilus for Kabyle (about 700 translated strings, Tier
3); no macOS, no Microsoft, no Xfce (`_ignored/i18n/kab/`). Kabyle is a Berber (Amazigh) language of Algeria with a
notably active free-software localization community (Mozilla, GNOME, LibreOffice all ship Kabyle). Evidence verified
against the pile on 2026-06-20.

## Decisions to confirm with David

- **Script: Latin (the GNOME/community standard), recommended; Tifinagh and Arabic also exist (high).** Kabyle is
  written today mainly in a Latin-based Berber orthography (the script of all the software localization, including the
  GNOME catalog here), and also in the traditional Tifinagh script and occasionally Arabic. Recommendation: **target
  Latin**, matching the community's de facto standard. Tifinagh would be a separate locale only if asked. Confidence:
  high.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Lean on the GNOME Kabyle catalog for established file-manager phrasing;
the Kabyle open-source localization community (around Mozilla and GNOME) is the de facto terminology authority. Error
messages stay calm and actionable: name the problem and the next step, and avoid a bare "tuccḍa" (error) status label,
consistent with Cmdr's English voice.

## Formality

- **Direct second person.** Kabyle second-person address is marked for gender and number, not for a formal/informal
  politeness split the way French is. Software addresses the user directly, following the GNOME catalog's conventions.
  For gender-neutral address, prefer impersonal or plural phrasing to avoid forcing masculine/feminine (see Decision
  points). Confidence: medium-high (community catalog, no MS style guide to confirm).
- **Action labels (buttons, menu items): the established form** from the GNOME catalog ("Semmet" Cancel, "Nɣel" Copy).
  Keep labels short. Confidence: medium-high.

## Decision points

- **Script: Latin Berber orthography, no decision for the shipped locale (high).** See Decisions to confirm. The Latin
  Kabyle alphabet uses special letters (ǧ, ɣ, ḥ, ṣ, ṭ, ḍ, ẓ, č, etc.). Confidence: high.
- **Regional variant: one, `kab` (Algeria).** Kabyle is one Berber variety (distinct from Tashelhit, Tarifit, and other
  Amazigh languages, which are separate codes). No internal regional split worth a matrix. Confidence: high.
- **Gender / inclusive language (medium problem, clean fix).** Kabyle grammar is gendered: the second person and verb
  agreement mark masculine vs feminine. Addressing a user of unknown gender forces a guess. **Fix: prefer impersonal or
  agentless phrasing** for system messages, and plural/neutral forms where possible. A native reviewer settles the
  details. Confidence: medium-high on the problem; the fix mirrors the gendered European languages.
- **Capitalization: sentence case (high).** Latin Kabyle has case; capitalize only the first word and proper nouns.
  English title case is wrong. Confidence: high.
- **State annexation (free vs construct state) affects placeholder grammar (high, Kabyle-specific).** Berber nouns
  change form between the "free state" and the "annexed/construct state" depending on syntactic position (e.g. after a
  preposition or as a postverbal subject). A fixed word around a `{placeholder}` may need a state it can't predict from
  runtime text. Structure sentences so a placeholder doesn't force a state change on an adjacent fixed word; a native
  reviewer handles annexation. Confidence: high; the subtlest translator-craft concern for Kabyle.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Only source for `kab` is GNOME Nautilus (Tier 3); the Kabyle
open-source community is the practical terminology authority. Mark single-source terms `tentative`/`high` per how
established they are. Sources decide the term; Cmdr writes its own value (GNOME GPL, never copied verbatim).

Settled from the GNOME Kabyle catalog (well-established community terms):

- **folder: `Akaram`** · GNOME ("Folder" → "Akaram"). `high`.
- **file: `Afaylu`** · GNOME ("File" → "Afaylu"). `high`.
- **trash: `Iḍumman`** · GNOME ("Trash" → "Iḍumman"). `high`.
- **copy: `Nɣel`** · GNOME ("Copy" → "Nɣel"). `high`.
- **cancel: `Semmet`** · GNOME ("Cancel" → "Semmet"). `high`.

Tentative / needs a native check:

- **open, delete, rename, eject, network, volume, pane, tab** · triangulate `kab/gnome-nautilus/`; where the GNOME
  catalog is thin, lean on the wider Kabyle Mozilla/GNOME corpus and native review. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR categories for `kab`: `one`, `other` (verified with `new Intl.PluralRules('kab')`, 2026-06-20). The GNOME Kabyle
catalog declares `nplurals=2; plural=n>1` (verified 2026-06-20); note this puts 0 and 1 together in the first form
(`n>1` is false for 0 and 1), a slightly different boundary than CLDR's `one`=1 only. Cmdr follows CLDR's `one`/`other`
for the check; a native reviewer confirms which counts truly share a form. Write both.

- **one**: integer 1 (CLDR). "1 afaylu".
- **other**: everything else, including 0 and counts ≥ 2. The `desktop-i18n-plural` check requires both. Kabyle has rich
  noun plural morphology (often internal/broken plurals); the native reviewer settles the counted-noun form.

## Notes and decisions

- **Quotation marks:** Kabyle UI commonly follows French guillemets `«…»` or English `"…"`; a native reviewer settles
  house style.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` follow the locale; never
  hardcode separators in a string.
- **Length.** Kabyle runs roughly English-length; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/kab/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
