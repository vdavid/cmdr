# Tatar (tt) translation style guide

Working notes for translating Cmdr into Tatar. Read [`README.md`](../README.md) for how this fits the translation
process, and the agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) for the ICU
mechanics every translator must follow.

**Priority signal: low.** Tatar localization is sparse. Apple does NOT ship Tatar (no macOS reference), and there is no
GNOME/Xfce Tatar file-manager catalog in the reference pile. The only authoritative source is Microsoft (terminology TBX
plus a 2011 style guide), both Cyrillic-only (`tt-Cyrl`). There is no Tier-1 macOS evidence, so most terminology calls
rest on Microsoft alone and stay tentative until a native reviewer signs off. Treat Tatar as a later-tier locale.

## Voice and tone

- Cmdr's English voice is friendly, concise, active, and never alarmist. Tatar should mirror that warmth but in a
  slightly more reserved register: Microsoft's Tatar style guide prescribes a **formal tone**, standard literary
  language, and no slang, colloquialisms, dialect words, or poetic phrasing.
- **No exclamation marks.** The Microsoft style guide explicitly says to avoid carrying over English exclamation marks.
  This aligns with Cmdr's never-alarmist rule, so drop them even where the English has one.
- The guide says passive voice is sometimes preferable to active in Tatar even where English is active. Cmdr defaults to
  active voice; keep active where it reads naturally, but don't force it if the natural Tatar phrasing is passive (for
  example impersonal system messages).
- Error copy: keep it calm and actionable, and avoid the equivalents of "error" and "failed" the way the English does.
  Note `error` itself translates as `хата`, but Cmdr's house rule is to phrase around it, not lead with it.

## Formality

- **Use the formal (V) second person: сез / -сез plural-respectful forms.** This is explicit in the Microsoft style
  guide: "When addressing the user, you should use plural respectful verb forms and pronouns." Never the familiar син /
  -сың. Example from the guide's own message corpus: "Бу тоташуны рөхсәт итәсезме?" (the -сез ending).
- Tatar is Turkic and agglutinative, so imperatives in buttons and menu items are verbal nouns or polite imperative
  forms, not bare stems. The MS terminology renders UI verbs as verbal nouns (the -у / -ү form): copy = `күчереп алу`,
  save = `саклау`, search = `эзләү`, open = `ачу`, move = `күчерү`, delete = `бетерү`. Follow that convention for action
  labels.
- **Agglutination on placeholders is a blind-translation risk (same family as Turkish).** A suffix often attaches to the
  noun a `{placeholder}` stands for (case endings, the question particle -мы/-ме, possessives), and the suffix's vowel
  must harmonize with the inserted word, which Cmdr cannot control. Structure sentences so a raw `{path}`, `{name}`, or
  `{message}` does not need a suffix glued to it: prefer a colon, a separate clause, or a fixed carrier noun that takes
  the suffix instead of the variable. Never assume the length, script, or final vowel of an inserted value.

## Decision points

### Script: Cyrillic (`tt-Cyrl`), the only realistic target

- **Options.** Cyrillic (official in Tatarstan/Russia, where the overwhelming majority of speakers live) versus the
  Latin "Zamanälif" alphabet (diaspora use in Turkey, the US, Australia, and Europe; historically decreed in Tatarstan
  in 1999 but overruled federally in 2002, so never official). An Arabic script also exists (China) but is irrelevant
  here.
- **How majors handle it.** Microsoft localizes Tatar in Cyrillic only (`tt-Cyrl`); its terminology and style guide
  carry no Latin variant. Apple ships no Tatar at all. There is no mainstream Latin-script software localization to lean
  on.
- **Recommendation: target Cyrillic, and tag the locale `tt`** (base, not `tt-Cyrl`). Base `tt` resolves to Cyrillic in
  practice (CLDR, Microsoft, and real-world software all default `tt` to Cyrillic), so a Latin `tt-Latn` is the variant
  that would need an explicit tag, not the reverse. Don't attempt a Latin translation unless David specifically wants to
  serve the diaspora. Confidence: **high.**

### Russian loanwords vs native Tatar coinages (the central terminology question)

- **The split is real and visible in the Microsoft glossary.** Tatar tech vocabulary borrows heavily from Russian, but
  the Tatar terminology body has coined native equivalents for many core concepts, and Microsoft uses a mix:
  - Russian loanwords kept: folder = `папка`, file = `файл`, disk = `диск`, server = `сервер`, directory = `каталог`,
    program = `программа`. (Microsoft tags these `geographicalUsage: RUS` in the TBX.)
  - Native Tatar coinages: computer = `санак`, drive = `туплагыч`, menu = `сайлак`, tab = `салынма`, window = `тәрәз`,
    plus the verbal-noun actions above (`саклау`, `эзләү`, `ачу`, `күчерү`, `бетерү`).
- **The decision.** For each recurring term, choose between the familiar Russian loan (what users likely say day to day)
  and the native coinage (what the Tatar literary/academic standard and Microsoft often prefer). These pull in opposite
  directions: loanwords maximize instant recognition; coinages match the formal literary register the style guide
  demands and read as more genuinely Tatar.
- **Recommendation: follow Microsoft's term-by-term choice as the default**, because it already encodes this tradeoff
  per word and it's the only authoritative source Tatar has. That means keeping `папка`/`файл`/`диск` as loanwords while
  using `санак`/`сайлак`/`тәрәз` natively. Don't impose a blanket "always native" or "always Russian" rule. Confidence:
  **tentative** per term (single source, no macOS cross-check, no native review). Push the whole glossary to David / a
  native reviewer before shipping.
- **Flag for David:** this is the one call that defines how "Tatar" Cmdr feels, and it can't be settled from Microsoft
  alone. Decide the house lean (track Microsoft, or push harder toward native coinages for a more distinctly Tatar
  product) before a full pass.

### Gender and inclusive language: not applicable

- Tatar is Turkic with **no grammatical gender** and no gendered pronouns (the Microsoft style guide's Gender section
  reads only "This section does not apply to Tatar"). This removes a whole class of agreement problems that plague
  Romance and Slavic locales. No gender-neutral workarounds needed. Confidence: **high.**

### Capitalization

- Tatar capitalizes sparingly: "capitalize only when you have to." English source capitalization does NOT carry over;
  many English-capitalized UI nouns and verbs are lowercase in Tatar. This matches Cmdr's sentence-case house rule, so
  the two agree. Confidence: **high.**

### Number and date formatting (Russia conventions)

- `Intl` for `tt` (verified on Node 22, 2026-06-20): thousands separator is a **space** and decimal is a **comma**
  (`1 234 567,89`); dates are **dd.mm.yyyy** (`20.06.2026`). These follow Russian/Tatarstan conventions. Use the
  platform `Intl` formatters; don't hand-format. Month names are lowercase (the style guide notes January is sometimes
  `январь`, the Russian form). Confidence: **high.**

## Brand and do-not-translate

Keep verbatim (product/platform names, not words): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus
the `{system_settings}`-style tokens. Enforced by `desktop-i18n-dont-translate`; the curated list lives in
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

- CLDR plural categories for `tt`: **`one`, `other`** (verified via
  `new Intl.PluralRules('tt').resolvedOptions().pluralCategories`, Node 22, 2026-06-20). Same two-category shape as
  Turkish and English, so every plural message needs exactly an `one` and an `other` branch.
- Grammar note: with no grammatical gender and no case-by-count interaction beyond the standard `one`/`other` split,
  plurals are straightforward. As in other Turkic languages, a noun after an explicit numeral commonly stays in the
  singular form, so the `other` branch may not always carry a plural suffix on the counted noun. Write the branches by
  meaning rather than mechanically pluralizing.

## Notes and decisions

- **Inches:** the style guide allows the `"` mark for inches only when length-locked; otherwise spell it out. Unlikely
  to matter for a file manager.
- **Reference pile is Microsoft-only and Cyrillic-only.** No macOS (Apple ships no Tatar), no GNOME/Xfce. Triangulation
  is impossible here, so most terms carry `tentative` confidence until native review. When a term isn't in the Microsoft
  TBX, that's a flag to surface, not a gap to fill by guessing.
- **ICU mechanics** (catalog-level, easy to miss): double every apostrophe in a value (`'` becomes `''`), and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Decisions to confirm with David

- **The Russian-loanword vs native-coinage house lean** (the central decision point above). Settle the policy before a
  full glossary pass.
- **Whether to localize Tatar at all yet, given the low-priority signal** (Microsoft-only sources, no Apple, no native
  reviewer lined up). Every chosen term is `tentative` until a native speaker reviews it.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/tt/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
