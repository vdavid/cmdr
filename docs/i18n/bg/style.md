# Bulgarian (bg) translation style guide

Working notes for translating Cmdr into Bulgarian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Bulgarian.

**No macOS reference.** Apple does NOT ship a Bulgarian macOS UI, so the pile has GNOME Nautilus + Xfce Thunar + MS
terminology + MS style guide only (`_ignored/i18n/bg/`); no macOS Finder. The highest-authority source (a real localized
OS) is absent, so terms lean on GNOME + Microsoft and stay a notch less certain than for a macOS-backed language.
Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. Only the first is a true open flag.

- **Address form: polite plural "Вие" recommended, needs a sign-off (high).** Bulgarian distinguishes formal plural
  "Вие" from informal singular "ти". Software convention and MS Bulgarian use the polite register (MS Bulgarian: "the
  second-person pronoun 'you' is used to politely ask the user", verified 2026-06-20). Recommended default: **polite
  plural throughout.** Flagging because Cmdr's English voice is warm-and-informal, so David may want to confirm the
  register shift is intended.
- **`volume`, `pane`, `tab` terms (tentative).** No macOS reference and GNOME doesn't cover all three cleanly; see the
  glossary. Worth a native check.

## Voice and tone

Friendly, concise, active, calm, but **polite in address** (Вие). MS Bulgarian says the Microsoft voice "avoids
old-fashioned, formal, and archaic words and expressions, which can sound unfriendly", prefers brief complete syntax,
and uses present tense (verified 2026-06-20). So keep it modern and plain, just polite. Error messages stay calm and
actionable: phrase the problem and the next step, and avoid "грешка" (error) / "неуспешно" (failed) as a bare status
label the way English avoids "error"/"failed". (MS Bulgarian notes passive voice is acceptable in error messages to
avoid blaming the user, verified 2026-06-20 - useful for phrasing failures gently.)

## Formality

- **Polite plural "Вие", throughout. Never informal "ти".** The polite form is carried by the plural verb ending and,
  when spelled out, the capitalized "Вие"/"Ви"/"Вас" in direct address to the user.
- **Action labels (buttons, menu items): verbal noun, not imperative.** The Bulgarian (and broader Slavic) UI norm, and
  what GNOME Bulgarian shows, is the verbal noun: "Преименуване" (renaming = Rename), "Изваждане" (ejecting = Eject),
  "Търсене" (searching = Search) (GNOME Nautilus, verified 2026-06-20). Avoid bare imperatives, which read as informal
  "ти".
- **Full sentences addressed to the user: polite plural.** "Наистина ли искате да изтриете тези файлове?" (Are you sure
  you want to delete these files?). So the rule is dual: **standalone labels = verbal noun; sentences to the user =
  polite plural.** Confidence: high.

## Decision points

- **Script: Cyrillic, no decision.** Bulgarian is written only in Cyrillic; there is no Latin variant in use.
  Confidence: high. (Bulgarian Cyrillic has a few letterform differences from Russian in italic faces, but that's font
  rendering, not orthography; nothing to encode in strings.)
- **Regional variant: one, `bg` (`bg-BG`).** Bulgarian is standardized only in Bulgaria; no second national standard, no
  variant matrix. Confidence: high.
- **Gender / inclusive language (high on the problem, high on the fix via polite plural).** Bulgarian past tense uses
  gendered participles (-л masc, -ла fem). A singular-addressed "you deleted" forces a gender guess, but the polite
  plural participle is `-ли`, which is **gender-neutral**: "Изтрихте 3 файла" works for any user. This is a second
  reason the polite plural is the right call. Where a singular adjective/participle would still agree with the user's
  gender, **rewrite impersonally**: "Копирането завърши" (Copying complete) or use the reflexive passive ("Файлът беше
  изтрит") rather than "Изтрихте…". Recommendation: lean on the gender-neutral polite plural for user actions, and
  impersonal phrasing for system-state messages. Confidence: high.
- **Definite article as a suffix (a Bulgarian-specific gotcha, high).** Bulgarian has no cases but marks definiteness
  with a suffixed article (файл → файлът / файла). A `{path}` or counted-noun insert that needs the definite form can't
  get it generically; structure sentences so the inserted value stays indefinite, or phrase around it. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Bulgarian capitalizes only the first word and proper nouns in
  titles, labels, and buttons. Matches Cmdr's sentence-case rule. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/bg/` (GNOME Nautilus, Xfce
Thunar, MS terminology, MS style guide; NO macOS) on 2026-06-20. Sources decide the term; Cmdr writes its own value (MS
copyrighted, GNOME/Xfce GPL, never copied verbatim). Without macOS, terms here are `high` only where GNOME and MS agree,
else `tentative`.

Settled terms (GNOME / MS agree):

- **folder: `папка`** · GNOME ("Папка"). `high`.
- **file: `файл`** · GNOME ("Файл"). Definite "файлът/файла". `high`.
- **trash: `кошче`** · GNOME ("Кошче"). `high`.
- **rename: `преименуване`** (noun label) / `преименувам` (verb) · GNOME ("Преименуване"). `high`.
- **eject: `изваждане`** · GNOME ("Изваждане"). `high`.
- **search: `търсене`** · GNOME ("Търсене"). `high`.
- **sidebar: `странична лента`** · GNOME ("Странична лента"). `high`.

Tentative / needs a native check:

- **volume: `дял` (partition) vs `том`** · no macOS reference; `дял` is the common partition/section term, `том` the
  literal "volume". Default `том` for a mounted disk; verify. `tentative`.
- **pane: `панел`** · the two file lists as "панели"; no direct GNOME "pane" term. `tentative`.
- **tab (UI tab): `раздел`** · MS/Slavic convention for UI tabs; verify against GNOME. `tentative`.
- **bookmark: `отметка`** · common Bulgarian term; verify. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand tokens sit inside Cyrillic text; that's normal, keep them Latin.

## Plurals

CLDR categories for `bg`: `one`, `other` (verified with `new Intl.PluralRules('bg')`). Only two forms. But the counted
noun after a number takes the **count plural** form, and masculine nouns have a distinct numeral-plural ("2 файла" vs
plain plural "файлове") - write the form that follows a number inside the count message. The `desktop-i18n-plural` check
requires both categories.

## Notes and decisions

- **Quotation marks: `„…"`** (low-9 opening U+201E, high-6 closing U+201C), the standard Bulgarian form (same shape as
  German/Czech). Avoid straight ASCII `"` and English `"…"`.
- **Numbers and dates come from the formatter layer.** Bulgarian uses a comma decimal and a space thousands separator (1
  000,5); `formatNumber()`/`formatBytes()` produce these. Never hardcode separators in a string.
- **Length.** Bulgarian runs somewhat longer than English; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/bg/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
