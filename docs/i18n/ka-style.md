# Georgian (ka) translation style guide

Working notes for translating Cmdr into Georgian (ქართული). Read [`README.md`](README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes
carry into Georgian.

Well-sourced for terms: the pile has MS terminology, MS style guide, GNOME Nautilus, and Xfce Thunar
(`_ignored/i18n/ka/`); no macOS folder for Georgian. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **`volume` and a few file-manager terms (tentative).** No macOS Georgian to anchor the highest-authority term;
  GNOME/MS are the sources. The settled list below is high-confidence where GNOME and MS agree; the tentative rows want
  a native check.

## Voice and tone

Friendly, concise, active, calm, never alarmist. MS Georgian targets a conversational, everyday register over formal
technical language (verified 2026-06-20), which matches Cmdr's English voice well. Georgian has no grammatical gender,
which removes a whole class of agreement problems (see Decision points). Error messages stay calm and actionable: name
the problem and the next step, and avoid a bare "შეცდომა" (error) label the way English avoids "error"/"failed".

## Formality

- **No T/V distinction to resolve. Georgian uses verb forms and the plural pronoun `თქვენ` for polite address.**
  Georgian doesn't have a French-style tu/vous split; politeness is carried by the plural-form verb and `თქვენ` (you,
  plural/polite). Standard software register addresses the user with the polite plural form. Confidence: high.
- **Action labels (buttons, menu items): use the established GNOME verbal-noun / imperative form.** macOS isn't
  available, so follow GNOME Nautilus, which uses verbal nouns for actions: "კოპირება" (copying/Copy), "გახსნა"
  (opening/Open), "გაუქმება" (cancelling/Cancel), "გამოღება" (ejecting/Eject) (GNOME, verified 2026-06-20). Keep this
  nominalized style for standalone action labels; use full polite-plural verb forms in sentences to the user.

## Decision points

- **Script: Georgian (Mkhedruli), no decision.** Georgian is written in its own unicameral Mkhedruli script. There is
  no case (no capital/lowercase), so the English title-case-vs-sentence-case question collapses: there is simply one
  letterform. Confidence: confirmed.
- **Regional variant: one, `ka` (`ka-GE`).** Georgian is standardized only in Georgia; no second national standard, no
  variant matrix. Confidence: high.
- **Gender / inclusive language: a non-issue.** Georgian has no grammatical gender and no gendered pronouns (a single
  third-person pronoun `ის` covers he/she/it). No gender guessing, no inclusive-form workarounds needed. This is one of
  the easier languages on this axis. Confidence: confirmed.
- **Capitalization: not applicable.** Mkhedruli has no letter case, so "sentence case" and "title case" don't exist as
  a choice. Don't try to capitalize the first letter of a label; there's nothing to capitalize. Confidence: confirmed.
- **Postpositions and agglutination affect placeholder grammar (high).** Georgian is agglutinative and uses
  postpositions (suffixes) rather than prepositions, and nouns take case suffixes. A `{path}` or `{name}` inserted
  before a postposition may need the postposition to attach to runtime text it can't control. Structure sentences so a
  placeholder lands in a position where the surrounding grammar doesn't depend on the inserted value's ending. A native
  reviewer handles the case-agreement edges. Confidence: high; the subtlest translator-craft concern for Georgian.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/ka/` (MS terminology, GNOME
Nautilus, Xfce Thunar) on 2026-06-20; no macOS for Georgian, so GNOME is the highest available authority for
file-manager terms. Sources decide the term; Cmdr writes its own value (MS copyrighted, GNOME/Xfce GPL, never copied
verbatim).

Settled terms (GNOME, cross-checked with MS where present):

- **folder: `საქაღალდე`** · GNOME ("Folder" → "საქაღალდე"). `high`.
- **file: `ფაილი`** · GNOME ("File" → "ფაილი"). `high`.
- **trash: `ნაგავი`** (trash bin) · GNOME ("Trash" → "ნაგავი"). `high`.
- **copy: `კოპირება`** · GNOME ("Copy" → "კოპირება"). `high`.
- **open: `გახსნა`** · GNOME ("Open" → "გახსნა"). `high`.
- **cancel: `გაუქმება`** · GNOME ("Cancel" → "გაუქმება"). `high`.
- **eject: `გამოღება`** · GNOME ("Eject" → "გამოღება"). `high`.
- **rename: `სახელის გადარქმევა`** · GNOME ("Rename" → "სახელის გადარქმევა"). `high`.

Tentative / needs a native check:

- **delete (permanent): confirm the GNOME/MS Georgian term** · the simple GNOME lookup didn't return a clean single
  string; triangulate "Delete" vs "Move to trash" before settling. `tentative`.
- **volume: needs a source** · no macOS anchor; check MS terminology for the storage-volume sense. `tentative`.
- **pane / tab: GNOME convention** · the two file lists and the UI tabs; confirm against GNOME window-region terms.
  `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand names stay in Latin script inside Georgian text.

## Plurals

CLDR categories for `ka`: `one`, `other` (verified with `new Intl.PluralRules('ka')`, 2026-06-20). Write both.

- **one**: integer 1 and decimals/numbers ending in a way CLDR maps to `one` for Georgian; in practice "1 ფაილი".
- **other**: everything else, including 0 and all counts ≥ 2: "5 ფაილი", "0 ფაილი". Note Georgian doesn't pluralize the
  counted noun after a number the way English does (the noun often stays in the singular form after a numeral), so the
  `other` branch typically keeps the singular noun. A native reviewer confirms the exact phrasing. The
  `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Quotation marks: `„…“`** (low-9 opening U+201E, high-6 closing U+201C) is the standard Georgian form, matching the
  German shape. Avoid straight ASCII `"` and English `"…"`.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` produce locale-correct
  separators; never hardcode them in a string.
- **Length.** Georgian can run longer than English; overflow-check the layout against the pseudolocale (`en-XA`).
  Mkhedruli also renders taller than Latin in some fonts, so check vertical fit too.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.
