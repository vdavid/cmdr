# Belarusian (be) translation style guide

Working notes for translating Cmdr into Belarusian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Belarusian.

**No macOS reference.** Apple does NOT ship a Belarusian macOS UI, so the pile has GNOME Nautilus + Xfce Thunar + MS
terminology + MS style guide for `be` (Cyrillic), plus a separate `be-Latn` GNOME catalog for the Latin (Łacinka) script
(`_ignored/i18n/be/` and `be-Latn/`). No macOS Finder. Terms lean on GNOME + Microsoft. Evidence verified against the
pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone.

- **Script and orthography: RESOLVED to Cyrillic, official наркамаўка orthography** (not classical тарашкевіца;
  `be-Latn` Łacinka out of scope). See the script decision points below and
  [`script-decisions.md`](../script-decisions.md). No longer open.
- **Address form: polite plural "Вы" recommended, worth a sign-off (high).** Like Russian/Bulgarian, software uses the
  polite plural. See Formality. Recommended default below.

## Voice and tone

Friendly, concise, active, calm, but **polite in address** (Вы). MS Belarusian follows the general Microsoft voice
("warm and relaxed, less formal, avoids unnecessarily formal tone", verified 2026-06-20), so keep it modern and plain,
just polite. With no macOS reference, prioritize clear, plain Belarusian. Error messages stay calm and actionable:
phrase the problem and the next step, and avoid "памылка" (error) / "не ўдалося" (failed) as a bare status label the way
English avoids "error"/"failed".

## Formality

- **Polite plural "Вы", throughout. Never informal singular "ты".** Carried by the plural verb ending; capitalized "Вы"
  in direct address.
- **Action labels (buttons, menu items): infinitive or verbal noun, not bare imperative.** GNOME Belarusian uses
  infinitives/verbal forms: "Перайменаваць" (Rename), "Выняць" (Eject), "Пошук" (Search), "Бакавая панэль" (Sidebar)
  (GNOME Nautilus, verified 2026-06-20). Avoid bare imperatives, which read as informal "ты".
- **Full sentences addressed to the user: polite plural.** So the rule is dual: **standalone labels = infinitive/verbal
  noun; sentences to the user = polite plural.** Confidence: high.

## Decision points

- **Orthography (наркамаўка vs тарашкевіца): RESOLVED to official наркамаўка.** The single most consequential decision
  for Belarusian, more than script. Recorded in [`script-decisions.md`](../script-decisions.md).
- **Script: RESOLVED to Cyrillic (`be`)**, with the Łacinka sibling (`be-Latn`) out of scope. Recorded in
  [`script-decisions.md`](../script-decisions.md).
- **Regional variant: one, `be` (`be-BY`).** Belarusian is standardized in Belarus; no second national standard. The
  meaningful split is orthographic (above), not regional. Confidence: high.
- **Gender / inclusive language (high on the problem, high on the fix via polite plural).** Belarusian past tense uses
  gendered l-participles (-ў/-ла). A singular-addressed "you deleted" forces a gender guess, but the polite plural
  participle is `-лі`, **gender-neutral**: "Выдалілі 3 файлы" works for any user. A second reason the polite plural is
  right. Where a singular adjective/participle would still agree, **rewrite impersonally** ("Капіяванне завершана", or
  the reflexive passive). Recommendation: lean on the gender-neutral polite plural for user actions, impersonal phrasing
  for system-state messages. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Belarusian capitalizes only the first word and proper nouns in
  titles, labels, and buttons. Matches Cmdr's sentence-case rule. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/be/` (GNOME Nautilus, Xfce
Thunar, MS terminology, MS style guide; NO macOS) on 2026-06-20. Sources decide the term; Cmdr writes its own value (MS
copyrighted, GNOME/Xfce GPL, never copied verbatim). Without macOS, terms are `high` where GNOME and MS agree, else
`tentative`. All spellings below assume the official orthography (see the flag).

Settled terms (GNOME / MS agree):

- **folder: `папка`** · GNOME ("Папка"). `high`.
- **file: `файл`** · GNOME ("Файл"). `high`.
- **trash: `сметніца`** · GNOME ("Сметніца"). `high`.
- **rename: `перайменаваць`** · GNOME ("Перайменаваць"). `high`.
- **eject: `выняць`** · GNOME ("Выняць"). `high`.
- **search: `пошук`** · GNOME ("Пошук"). `high`.
- **sidebar: `бакавая панэль`** · GNOME ("Бакавая панэль"). `high`.

Tentative / needs a native check:

- **volume: `том` (literal) vs `раздзел` (partition)** · no macOS reference; default `том` for a mounted disk; verify.
  `tentative`.
- **pane: `панэль`** · the two file lists as "панэлі". `tentative`.
- **tab (UI tab): `укладка`** · common Slavic UI-tab term; verify against GNOME. `tentative`.
- **bookmark: `закладка`** · common term; verify. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand tokens sit inside Cyrillic text; keep them Latin.

## Plurals

CLDR categories for `be`: `one`, `few`, `many`, `other` (verified with `new Intl.PluralRules('be')`). Write all four.
The East-Slavic pattern (same shape as Russian/Ukrainian):

- **one**: numbers ending in 1 but not 11 (1, 21, 31, … "1 файл").
- **few**: numbers ending in 2-4 but not 12-14 (2, 3, 4, 22, … "2 файлы").
- **many**: numbers ending in 0, 5-9, or 11-14 (0, 5, 11, 12, 100, … "5 файлаў").
- **other**: the decimal/fraction bucket ("1,5 файла").
- **Trap: this is the East-Slavic mod-10/mod-100 rule, NOT the West-Slavic (Czech/Slovak) one.** Here `many` is the
  big-number bucket (5-9, teens) and `other` is the decimal bucket - the OPPOSITE of Czech, where `many` is decimals.
  Don't copy a Czech plural structure into Belarusian.
- Forms map to cases: 1 = nominative sg, 2-4 = genitive sg (Belarusian/Russian pattern), 5+ = genitive pl. Keep
  agreement inside each branch. The `desktop-i18n-plural` check requires all four.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets, the standard East-Slavic form, same as Russian). Nested: `„…"`. Avoid straight
  ASCII `"`.
- **Numbers and dates come from the formatter layer.** Belarusian uses a comma decimal and space thousands separator (1
  000,5); `formatNumber()`/`formatBytes()` produce these. Never hardcode separators in a string.
- **Length.** Belarusian runs somewhat longer than English; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Belarusian uses an apostrophe-like character (the separating sign, often typed
  as `'` U+0027 or `’`) inside words ("аб'ём") - that real apostrophe must be DOUBLED in ICU values too, a common
  Belarusian-specific trap. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/be/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
