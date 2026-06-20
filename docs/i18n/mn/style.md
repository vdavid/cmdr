# Mongolian (mn) translation style guide

Working notes for translating Cmdr into Mongolian. Read [`README.md`](../README.md) for how this fits the translation
process. `mn` here means **Mongolian in Cyrillic script** (the practical software default; see Decision points). The
base tag is `mn`; a `mn-Cyrl` variant is unnecessary unless a `mn-Mong` (Traditional script) locale ever ships
alongside it.

## Voice and tone

Cmdr's Mongolian voice mirrors its English one: friendly, concise, active, and never alarmist. This matches the modern
Mongolian software register that Microsoft codifies for Mongolian (Cyrillic): warm and conversational, plain everyday
words over formal or bookish ones, short sentences, sentence fragments where they read naturally.

- Prefer the modern everyday word over the older formal one. Microsoft's own list: `хийх` not `зорилгод хүрэх`
  (achieve), `туслах` not `мэдээллээр хангах` (provide info), `одоо` not `өгөгдсөн хугацаанд` (now). Pick the short,
  spoken form.
- Stay calm and actionable in error messages, and keep the English rule of avoiding the words "error" and "failed".
  Describe what happened and what to do, not that something failed. Mongolian phrases these naturally as a neutral
  statement plus a next step (Microsoft's pattern: "Нууц үг буруу тул дахиад оролдоод үз" = the password is wrong, so
  try again).
- Drop English filler that carries no meaning ("successfully", "please" as a verbal tic). The Mongolian sentence states
  the outcome on its own.

## Formality

This is the single biggest tone rule for Mongolian, and it inverts the English instinct.

- **Avoid the second-person pronoun.** English leans on "you / your" constantly; Mongolian does not. Microsoft's
  Mongolian guide is explicit: avoid `та` / `таны` / `танд` wherever possible, and use neutral structures or passive
  voice instead. Translating every English "you" literally produces stiff, machine-translated Mongolian. Example
  (Microsoft): "You cannot select any names now" becomes "Одоо ямар ч нэр сонгох боломжгүй" (no `та`), not "Та одоо ...".
- **Use `та` (the polite second person) only when direction genuinely needs it** and there's no clean neutral phrasing.
  When `та` does appear, it's the polite/plural form `та`, never the familiar `чи`. There is no informal-register
  decision to make: `чи` is wrong for app chrome.
- **UI actions use the imperative with the polite suffix.** For buttons, menu items, and instructions the user
  performs, use the imperative mood, typically the polite `-на уу` / `-гана уу` ending: "...суулгана уу" (install),
  "дахин оролдоно уу" (try again). Microsoft prefers the bare polite imperative ("суулгана уу") over the longer
  `...суулгах ёстой` ("you must install") construction. For very short button labels, the plain verb stem is fine
  (Cyrillic Nautilus and the Microsoft terminology use bare verbs: `хуулах` copy, `устгах` delete, `нэрлэх` rename).
- **System/program actions take neutral passive, no subject.** When Cmdr or the OS is the actor (a transfer, a failure,
  a background task), use a subjectless passive construction, not "the app did X" and not "you".

## Decision points

The genuinely tricky calls, with how the majors handle each, a recommended default, and a confidence level.

- **Script: RESOLVED to Cyrillic (`mn`), not Traditional Mongolian vertical script.** Recorded in
  [`script-decisions.md`](../script-decisions.md). The evidence below stands.
  Mongolian is written in two scripts: Cyrillic (dominant in independent Mongolia since the 1940s, the everyday script
  for ~3M people and every mainstream device) and the Traditional Mongolian vertical script `mn-Mong` (used in Inner
  Mongolia, China, and the target of a Mongolian-government revival that since Jan 2025 requires both scripts in
  official documents, though Cyrillic stays the predominant medium). For software the gap is decisive: a 2023 device
  survey found NO major OS had adequate Traditional-script localization (iOS has no Mongolian UI at all; Android had no
  Traditional display-language option; Windows 10 offered "Mongolian (Traditional)" but rendered a mix of horizontal
  Traditional, Cyrillic, and English). Microsoft ships a Mongolian **(Cyrillic)** terminology set and style guide, no
  equivalent Traditional one; the reference pile likewise has a Cyrillic Microsoft style guide and a Cyrillic Nautilus
  catalog, nothing in Traditional. Traditional script also demands vertical text layout plus complex cursive shaping, a
  real and large engineering burden Cmdr's Tauri/Svelte UI is not built for. Recommendation: ship **Cyrillic** as `mn`.
  Confidence: high on Cyrillic being the right practical default; the decision to support Mongolian at all, and to ever
  attempt Traditional, is David's.

- **Formality: polite, but pronoun-light.** Covered above. Default to neutral/passive phrasing, polite `та` only when
  unavoidable, polite imperative for actions, never `чи`. Recommendation: as in the Formality section. Confidence: high
  (this is Microsoft's documented Mongolian-Cyrillic rule, not a guess).

- **Anglicism vs native words: prefer the established native term; the file-manager core has good ones.** Mongolian
  Cyrillic has settled native vocabulary for the domain, and both Microsoft and GNOME use it: `файл` (file, itself a
  long-naturalized loan and the standard term), `хавтас` (folder; Microsoft and Nautilus agree, not the colloquial
  loan `фолдер`), `хуулах` (copy), `устгах` (delete), `нэрлэх` / `нэр` (rename / name), `систем` (system). The judgment
  cases are newer or compound terms (pane, tab, volume, mount) where usage is thinner; coin or borrow consistently and
  record each in the glossary. Recommendation: use the established native term for the core vocabulary; avoid casual
  loans like `фолдер` where `хавтас` is standard; lock the thinner terms in the glossary as they come up. Confidence:
  high for the core, medium for the newer terms. Flag for David: "pane" has no settled Mongolian term and needs one
  picked and locked.

- **Compounds: separate words or one word, NO hyphens.** Microsoft's guide is explicit that English hyphenated/inverted
  compounds should NOT carry the hyphen into Mongolian: write "анх удаа суурилуулах" (first-time setup), not
  "анх-удаа"; "хэрэглэгчийн тодорхойлсон параметр" (user-specified parameter), not "хэрэглэгчийн-тодорхойлсон". This is
  a frequent mechanical error worth a dedicated review pass. Recommendation: drop hyphens from translated compounds.
  Confidence: high.

- **Punctuation and symbols.** No em-dashes: Microsoft says the em-dash is not applicable to Mongolian and must become a
  colon, semicolon, parentheses, or a reworded sentence (this matches Cmdr's own no-em-dash rule). En-dash for numeric
  ranges with no surrounding spaces ("Хуудас 3–5") and as a minus sign with no space after it ("−5"). No comma between
  subject and predicate (incorrect in Mongolian), and no comma after the conjunctions `ба` / `бөгөөд` / `эсвэл` /
  `буюу` / `болон`. The symbols `@`, `#`, and `&` are not used in Mongolian body text: write `ба` for "and" (not `&`),
  and `зүйлийн тоо` for "# items". Recommendation: follow these; most are handled in copy, not code. Confidence: high.

- **Quotation marks: straight double quotes in the UI, never single.** For software, product help, and web pages,
  Microsoft Mongolian uses straight double quotes `"..."` (the same as English) and explicitly says DO NOT use single
  quotation marks. Chevrons «...» are for documentation only, not app chrome. Quote UI item names (menus, commands)
  when they lack other formatting. Product/brand names are NOT quoted. Recommendation: `"..."` in the app, no single
  quotes, no chevrons in UI strings. Confidence: high.

- **Inclusive/gendered language.** Mongolian has no grammatical gender and the pronoun-light style means the user is
  rarely addressed by a gendered role anyway. No special handling needed. Recommendation: none. Confidence: high
  (low-stakes).

- **Numbers and dates.** Mongolian uses a space (or non-breaking space) as the thousands separator; `Intl` handles this
  at runtime, so it matters only for any hand-written numeral in copy. Times use the 24-hour clock with a colon
  ("18:30", per Microsoft). ISO dates (YYYY-MM-DD) per Cmdr's house rule. Recommendation: rely on `Intl`; 24-hour
  times. Confidence: high.

## Terminology and glossary

A few core terms confirmed against Microsoft Mongolian (Cyrillic) terminology and GNOME Nautilus (`mn`). Extend as
strings come up; record every newly-coined term so it stays consistent.

| English term | Mongolian | Notes |
| ------------ | --------- | ----- |
| file | файл | naturalized loan, the standard term |
| folder | хавтас | not the casual loan `фолдер` |
| copy | хуулах | bare verb for buttons |
| move | зөөх | |
| delete | устгах | |
| rename | нэрлэх / нэр өөрчлөх | `нэр` = name |
| open | нээх | |
| cancel | болих / цуцлах | confirm against the catalog as it grows |
| system | систем | |
| settings | тохиргоо | |
| search | хайх | Microsoft fragment uses `Хай` |
| tab | (lock in glossary) | UI tab; pick one term and keep it |
| pane | (unresolved - David to pick) | no settled Mongolian term; flag |
| volume | (lock in glossary) | storage volume sense |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `mn`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('mn').resolvedOptions().pluralCategories`; `mn-Cyrl` returns the same). Same two-category shape as
English, so every plural message needs both branches. Note that Mongolian often leaves the counted noun in the singular
even after a number, so write each branch as a full natural phrase rather than swapping only the numeral. The
`desktop-i18n-plural` check requires both categories.

## Notes and decisions

- Script: RESOLVED to Cyrillic (`mn`); Traditional vertical script (`mn-Mong`) is out of scope. See
  [`script-decisions.md`](../script-decisions.md).
- Pronouns: avoid `та` / `таны` / `танд`; use neutral or passive phrasing; polite `та` only when direction needs it;
  never `чи`.
- Actions: polite imperative (`-на уу`) for instructions the user performs; bare verb for short buttons; subjectless
  passive for system/program actions.
- Compounds: no hyphens; separate words or one word.
- Punctuation: no em-dashes (→ colon/semicolon/parentheses); en-dash for ranges and minus with no trailing space; no
  comma between subject and predicate; no comma after `ба`/`бөгөөд`/`эсвэл`/`буюу`/`болон`.
- Symbols: no `@` `#` `&` in body text (`ба` for "and", `зүйлийн тоо` for "# items").
- Quotation marks: straight `"..."` in the UI, never single quotes; chevrons «...» for docs only; brand names unquoted.
- Numbers/time: space thousands separator (via `Intl`); 24-hour times with colon; ISO dates.
- Dedicate one human review pass to pronoun-stripping (avoiding literal `та`) and to compound de-hyphenation, the two
  highest-frequency Mongolian UI errors.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/mn/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
