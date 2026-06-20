# Kazakh (kk) translation style guide

Working notes for translating Cmdr into Kazakh (қазақ тілі). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Kazakh.

Well-sourced for terms: the pile has MS terminology, MS style guide, GNOME Nautilus, and Xfce Thunar
(`_ignored/i18n/kk/`); no macOS folder for Kazakh. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **Script: RESOLVED to Cyrillic (base tag `kk`).** `kk-Latn` is a later fast-follow as the ~2031 Latin transition
  lands; building the catalog in `kk` (Cyrillic) now keeps the seam clean for adding `kk-Latn` later. See the script
  decision point below and [`script-decisions.md`](../script-decisions.md). No longer open.

## Voice and tone

Friendly, concise, active, calm, never alarmist. MS Kazakh targets a "clear, friendly, and concise" conversational
register and explicitly "avoids an unnecessarily formal tone" while addressing the user directly in the second person
(verified 2026-06-20), a good match for Cmdr's English voice. Error messages stay calm and actionable: name the
problem and the next step, and avoid a bare "қате" (error) status label the way English avoids "error"/"failed".

## Formality

- **Polite second person, addressing the user directly.** MS Kazakh uses the second-person pronoun "сіз" (polite you)
  to ask the user to take action, and prefers direct second-person/imperative phrasing (verified 2026-06-20). Kazakh
  distinguishes polite `сіз` from familiar `сен`; software uses `сіз`. Recommended default: **`сіз`-register
  throughout.** Confidence: high.
- **Action labels (buttons, menu items): the established GNOME verbal-noun / imperative form.** macOS isn't available;
  follow GNOME Nautilus: "Көшіру/Көшіріп алу" (Copy), "Ашу" (Open), "Бас тарту" (Cancel), "Шығару" (Eject), "Атын
  өзгерту" (Rename) (GNOME, verified 2026-06-20). Keep this style for standalone labels; use full `сіз`-form verbs in
  sentences to the user.

## Decision points

- **Script: RESOLVED to Cyrillic (base tag `kk`), Latin coming.** Kazakh is written today in a 42-letter Cyrillic
  alphabet; Kazakhstan has an official roadmap to switch to a Latin alphabet (the exact letterforms have been revised
  several times). Every authoritative source in the pile (MS, GNOME) is Cyrillic (verified 2026-06-20). Ship Cyrillic
  (`kk`) now; `kk-Latn` is a later fast-follow as the ~2031 transition lands, added as a sibling rather than reflowing
  the base. Don't pre-emptively translate to Latin. Recorded in [`script-decisions.md`](../script-decisions.md).
- **Regional variant: one, `kk` (`kk-KZ`).** Kazakh is standardized in Kazakhstan; no second national standard worth a
  variant matrix (the Kazakh diaspora in China/Mongolia uses other scripts but isn't a localization target here).
  Confidence: high.
- **Gender / inclusive language: a non-issue.** Kazakh (Turkic) has no grammatical gender and a single gender-neutral
  third-person pronoun (`ол`). No gender guessing, no inclusive-form workarounds. Confidence: confirmed.
- **Agglutination affects placeholder grammar (high).** Kazakh is agglutinative: nouns take stacked case/possessive
  suffixes, and the suffix form follows vowel harmony with the stem. A `{path}` or `{name}` placeholder followed by a
  case suffix can't reliably attach a harmonized suffix to runtime text. Structure sentences so a placeholder lands
  where the grammar doesn't depend on the inserted value's final vowel. A native reviewer handles vowel-harmony edges.
  Confidence: high; the subtlest translator-craft concern for Kazakh.
- **Capitalization: sentence case (high).** Kazakh Cyrillic has case; capitalize only the first word and proper nouns
  in labels and titles. English title case is wrong. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/kk/` (MS terminology, GNOME
Nautilus, Xfce Thunar) on 2026-06-20; no macOS for Kazakh, so GNOME is the highest available authority for file-manager
terms. Sources decide the term; Cmdr writes its own value (MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (GNOME, cross-checked with MS where present):

- **folder: `бума`** · GNOME ("Folder" → "Бума"). Note `бума` is the GNOME choice; MS may prefer `қалта`, so
  triangulate. `high`.
- **file: `файл`** · GNOME ("File" → "Файл"). `high`.
- **trash: `қоқыс шелегі`** (trash bin) · GNOME ("Trash" → "Қоқыс шелегі"). `high`.
- **copy: `көшіру` / `көшіріп алу`** · GNOME ("Copy"). `high`.
- **open: `ашу`** · GNOME ("Open" → "Ашу"). `high`.
- **cancel: `бас тарту`** · GNOME ("Cancel" → "Бас тарту"). `high`.
- **eject: `шығару`** · GNOME ("Eject" → "Шығару"). `high`.
- **rename: `атын өзгерту`** · GNOME ("Rename" → "Атын өзгерту"). `high`.

Tentative / needs a native check:

- **folder: `қалта` vs `бума`** · GNOME uses `бума`; `қалта` (pocket/folder) is also common in Kazakh UI. Settle which
  reads most natural for Cmdr. `tentative`.
- **volume: needs a source** · no macOS anchor; check MS terminology for the storage-volume sense. `tentative`.
- **pane / tab: GNOME convention** · confirm against GNOME window-region and tab terms. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand names stay in Latin script inside Cyrillic text.

## Plurals

CLDR categories for `kk`: `one`, `other` (verified with `new Intl.PluralRules('kk')`, 2026-06-20). Write both.

- **one**: integer 1 (and CLDR-mapped `one` cases). "1 файл".
- **other**: everything else, including 0 and all counts ≥ 2: "5 файл", "0 файл". As in other Turkic languages, the
  counted noun typically stays singular after a numeral (no plural suffix when a number is present), so the `other`
  branch keeps the singular noun. A native reviewer confirms. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets), the Cyrillic/Russian-influenced standard for Kazakh. Avoid straight ASCII
  `"` and English `"…"`.
- **Numbers and dates come from the formatter layer.** Kazakh uses a comma decimal and space thousands separator;
  `formatNumber()`/`formatBytes()` produce these from the locale. Never hardcode separators in a string.
- **Length.** Kazakh runs longer than English (agglutinative suffixes); overflow-check the layout against the
  pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/kk/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
