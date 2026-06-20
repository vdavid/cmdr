# Somali (so) translation style guide

Working notes for translating Cmdr into Somali. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Somali.

## Priority and coverage reality (read first)

Somali is a low-priority, low-resource target for a macOS app, and that shapes every call below.

- **Apple does not localize macOS into Somali.** Somali isn't among macOS's ~47 system languages, so there is no Finder,
  AppKit, or System Settings reference to mirror (Apple's own language list, checked 2026-06-20). The whole "prefer the
  macOS term" rule that anchors the Swedish guide has no anchor here.
- **The reference pile has only the Microsoft style guide** (no macOS, no Microsoft terminology `.tbx`, no
  GNOME/Nautilus, no Xfce/Thunar; verified against the reference pile, 2026-06-20). So term choices can't be
  triangulated across sources the way mature languages allow. Most terminology calls below are therefore `tentative` and
  should be flagged to David or a native reviewer rather than shipped on agent judgment.
- **Practical consequence:** treat Somali as a later-stage locale. Don't invent a full native IT vocabulary from
  scratch. Where no established Somali term exists, borrow the English word conservatively (see the borrowing decision
  point) rather than coining one, and leave the term `tentative` until a native reviewer signs off.

## Voice and tone

Friendly, warm, calm, concise. The Microsoft Somali guidance matches Cmdr's English voice well: "warm and relaxed, crisp
and clear", short easy-to-read sentences, no literal translation, conversational but not slangy (verified against the
reference pile, 2026-06-20). Keep one verb per sentence where you can; the MS guide explicitly asks for simple structure
read aloud as a screen reader would.

Error messages stay calm and actionable, naming the problem and the next step, and avoid framing anything as a failure.
Somali has no neat single word for "error" to avoid the way English does, so the guideline is behavioral: describe what
happened and what to try, don't lead with blame.

## Formality

- **Address the user as `adiga` (second person singular), lowercase in running text.** This is Microsoft's chosen form
  of address for Somali (verified against the reference pile, 2026-06-20), and it fits a personal desktop app the user
  owns.
- **Respect-by-plural exists and is real, but isn't the UI default.** Somali shows respect by switching to the plural
  `idinka` / plural verb endings (`-tihiin`, `-diin`) when addressing elders or people you don't know well. A file
  manager speaking to its single owner is the casual, direct register, so stay with `adiga`. Flag for David: if Cmdr's
  Somali voice should feel more deferential, the plural-respect form is the lever, but the MS-backed default is singular
  `adiga`. `tentative` (register call, not a term).
- **Buttons and menu items: imperative verb.** Somali imperatives are natural and short for actions ("Tirtir" delete,
  "Jooji" cancel/stop, "Nuqul" or "Koobi" copy). Confirm exact verbs with a native reviewer; the imperative _register_
  is the safe call, the specific verbs are `tentative`.
- **Avoid gendered pronouns in generic references.** The MS Somali guide is explicit: don't use `isaga`/`iyada` in
  generic UI text. Rewrite to second person (`adiga`), use a plural noun, or use the article instead of a possessive
  (their example: `dukumeentiga` rather than `dukumeentigiisa`). This matters because Somali grammatical gender would
  otherwise leak into neutral strings (verified against the reference pile, 2026-06-20).

## Decision points

These are the calls that actually move the needle for Somali. Each: how the majors handle it, a recommended default, a
confidence, and whether only David can settle it.

- **Translate vs borrow tech terms (the central decision).**
  - Somali is low-resource for software UI: there's no established, widely-recognized native vocabulary for "file",
    "folder", "pane", "tab", or "volume" that a Somali user would reliably recognize over the English word. Major
    vendors barely localize into Somali at all (Apple not at all; Google covers Somali in Search/Translate/Android to a
    limited degree; Microsoft has a style guide but the pile carries no Somali terminology glossary), so there's no
    settled reference catalog to copy.
  - Recommended default: **borrow English conservatively where no established Somali term exists**, and translate only
    the terms with a clear, everyday Somali word the user already knows (for example actions like delete/cancel/open,
    and common nouns). Don't coin neologisms. When borrowing, treat the loan as masculine (the MS guide states loan
    words are masculine in Somali) and keep the English spelling for product-adjacent nouns.
  - `tentative`, and a David/native-reviewer flag: the translate-vs-borrow line per term can't be drawn confidently
    without a native speaker. This is the single most important thing to settle before a real Somali pass.

- **Script and special letters: confirm, but Latin is correct.**
  - Standard Somali (Af Soomaali) is written in the **Somali Latin alphabet** (Latin script, official since 1972). No
    other script is in play for modern standard Somali. The alphabet uses plain Latin letters plus the digraphs `dh`,
    `kh`, `sh`, and the letter `c` (for the pharyngeal `ʿayn`) and `x` (for `ḥ`); it has no `p`, `v`, or `z` natively.
  - Recommendation: write in standard Somali Latin orthography; rely on the formatter layer for any locale-specific
    number shaping. No script switch or transliteration is needed. `high` (well-established fact, not a judgment call).

- **Regional variant: Somalia-standard, not a diaspora dialect.**
  - Somali has dialect variation (Northern/Standard, Benadiri, Maay), and a large diaspora (including in Sweden, where
    David lives). Standard Somali based on Northern Somali is the written norm across media and education and is what
    the MS guide targets.
  - Recommendation: target **Standard Somali** (`so`, no region subtag). Don't try to localize for a specific diaspora
    community. `high` for the standard-written choice; the diaspora has no separate written norm to split toward.

- **Gender and inclusive language.**
  - Somali has grammatical gender and gendered pronouns, which the MS guide says to keep out of generic UI references
    (see Formality). Loan words default to masculine.
  - Recommendation: follow the MS rewrite strategies (second person, plural noun, article over possessive) so neutral
    strings don't force a gender. `high` (directly sourced).

- **Length.**
  - Somali UI strings tend to run a bit longer than English (agglutinative morphology, definite-article suffixes,
    spelled-out connectors like `iyo` that the MS guide says not to abbreviate for screen readers). Borrowed English
    nouns keep length near English, which is one practical argument for conservative borrowing.
  - Recommendation: overflow-check the layout against the pseudolocale (`en-XA`) as for every language; expect modest
    expansion, not contraction. `tentative` (no measured corpus for Somali UI).

## Terminology and glossary

Deferred this round. No reliable multi-source anchor exists for Somali (only the MS style guide, no terminology glossary
or OS catalog), so terms should be filled in with a native reviewer rather than guessed. When started, use the same
`chosen · sources · confidence` shape as the Swedish guide, and expect most entries to start `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list is enforced by `desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.
For English acronyms the MS guide suggests expanding on first use with the acronym in parentheses, but in short UI
labels keep the acronym alone.

## Plurals

CLDR categories for `so`: `one`, `other` (run `new Intl.PluralRules('so').resolvedOptions().pluralCategories` to confirm
against the shipped ICU). Write both branches. Numbers and dates come from the formatter layer (`formatNumber()` /
`formatBytes()`); never hardcode separators in a string.

## Notes and decisions

- **Don't keep `&` as an ampersand in running text.** The MS Somali guide says translate `&` as `iyo` and spell out
  connectors, because screen readers misread `&`, `+`, and `~`. Keep `&` only inside tags, placeholders, or shortcut
  codes (verified against the reference pile, 2026-06-20).
- **Capitalization roughly follows English** per the MS guide (proper names, menu items, headings capitalized similarly
  to English). Cmdr's sentence-case rule still applies to its own labels; don't title-case.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/so/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
