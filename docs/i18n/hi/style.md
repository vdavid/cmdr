# Hindi (hi) translation style guide

Working notes for translating Cmdr into Hindi. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Hindi.

Hindi is a tier-1 well-localized language: Apple (Finder), Microsoft, Google, Spotify, and Netflix all ship Hindi, so
triangulation evidence is strong. Sources mined for this guide: macOS Finder/AppKit Hindi strings, the Microsoft Hindi
terminology and style guide, and the GNOME Nautilus Hindi catalog.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## Decisions to confirm with David

These are calls a translator can't make alone. The rest of this guide assumes them.

- **Native-Hindi vs English-loanword leaning, tentative.** Hindi UI vocabulary spans a Sanskritized "pure Hindi"
  register and an English-loanword register, and the choice sets the whole catalog's feel (see the tech-term decision
  point). macOS sits in the middle (native words where natural, English loans where they're what people actually say:
  `फ़ोल्डर`, `फ़ाइल`, `कॉपी`, `टैब`). Recommendation below follows macOS, but the register lean is worth David's glance.
- **Regional target: there's effectively one written Standard Hindi (India), low risk.** Hindi is overwhelmingly an
  India locale (`hi` / `hi-IN`); ship one `hi` catalog. Flagged only because picking a region is nominally a product
  call; the default is safe.

## Voice and tone

Cmdr's Hindi voice is friendly, concise, active, and never alarmist, matching the English. Microsoft's Hindi voice
guidance lines up with Cmdr's: "warm and relaxed, less formal," "crisp and clear, write for scanning first," everyday
conversational Hindi over formal/technical Hindi (verified against the reference pile,
`hi/microsoft-style-guides/StyleGuide.pdf`, 2026-06-20). Carry that over: short, spoken, modern Hindi, avoiding the
heavy Sanskritized register that reads as archaic.

Error and warning messages stay calm and actionable. Keep the English rule of avoiding the words "error" and "failed";
phrase what happened and the next step (Hindi has neutral framings around `नहीं हो सका` "couldn't…") rather than a loud
`त्रुटि` (error) / `विफल` (failed) label.

## Formality

- **Address the user as `आप` (respectful you), high.** Hindi has a three-way politeness split: `तू` (intimate/rude in
  UI), `तुम` (familiar), and `आप` (respectful/polite). macOS uses `आप` exclusively (617 instances, zero `तुम` or `तू`,
  verified against the reference pile, 2026-06-20), and Microsoft's Hindi style guide directs addressing the user
  politely. Always use `आप`; never `तू`/`तुम` in UI, which reads as curt or talking down. Keep verb endings in the `आप`
  register (the `-इए` / `-एँ` polite imperative) consistently.
- **Buttons and menu items: polite imperative.** macOS labels actions in the polite imperative: `कॉपी करें` (copy),
  `रद्द करें` (cancel), `हटाएँ` (delete), `खोलें` (open), `बाहर निकालें` (eject). Keep these concise polite-imperative
  forms; don't expand to full request sentences on buttons.

## Decision points

### Script: Devanagari, confirm native, never Latin/Romanized

- Hindi is written in the **Devanagari** script (an abugida), NOT Latin. Every major that localizes Hindi ships native
  Devanagari, never romanized "Hinglish": Apple (macOS Finder), Microsoft, Google, Spotify, and Netflix all render real
  Devanagari (macOS verified against the pile; Google/Spotify/Netflix web-evidenced, unverified). `high`.
- **Rendering care:** Devanagari uses combining vowel signs (matras), conjunct consonant clusters, the virama (halant),
  and the nukta (e.g. `फ़`, `ज़`). Some matras render before/above/below their base, so visual order isn't code-point
  order. Don't measure, truncate, or reverse strings by code unit, and confirm the app's font stack shapes Devanagari
  correctly (matras attach, conjuncts form, no tofu boxes). This is a real complex-script rendering risk Latin languages
  don't have.
- **Do NOT transliterate** product UI into Latin "Hinglish"; that's an informal chat register, never a localized-UI
  convention. (Hinglish in the sense of English loanwords written in Devanagari, e.g. `फ़ोल्डर`, IS fine and standard;
  the ban is on Latin-script Hindi.)

### Tech-term strategy: follow macOS's middle register, borrow where people borrow

- The genuine choice is Sanskritized native term vs naturalized English loanword. Pure-Hindi coinages (e.g. `संचिका` for
  file, `संगणक` for computer) read as archaic/officialese to most users; full English-in-Devanagari can read as lazy.
  macOS strikes the middle most users expect, and that's the recommended target. `tentative` on the overall lean
  (flagged above); individual terms below are `high`.
- macOS uses English-loanword Devanagari for the most-borrowed nouns (`फ़ोल्डर` folder, `फ़ाइल` file, `कॉपी` copy, `टैब`
  tab, `वॉल्यूम` volume) and native Hindi for verbs and some nouns (`हटाएँ` delete, `खोलें` open, `खोज` search,
  `बाहर निकालें` eject, `रद्दी` trash). Follow that pattern: don't force a Sanskrit coinage where macOS and everyday
  speech use the loan, and don't English-ify a word that has a natural, common Hindi form.

### Gender: grammatical gender pervades; keep UI gender-neutral toward the user

- Hindi has grammatical gender (masculine/feminine) that propagates through verb endings, adjectives, and participles,
  and second-person polite forms with `आप` are largely gender-neutral for the user (the polite plural agreement avoids
  marking the user's gender). `high`.
- Keep strings addressed to the user in the `आप` register so verb agreement stays neutral; refer to files/items by their
  (grammatically gendered) noun and make any adjective/verb agree with THAT noun, not with the user. Never write a
  string that has to guess the user's gender. Where a file's noun gender would force an agreement (e.g. `फ़ाइल` is
  feminine, `फ़ोल्डर` masculine), agree with the noun in that specific string.

### Numerals: Arabic (0-9), not Devanagari (०-९)

- Hindi has Devanagari digits (`०१२३४५६७८९`) but modern Hindi software and macOS use Arabic numerals (0-9) for counts,
  sizes, and percentages. macOS Hindi Finder uses Arabic digits throughout (verified against the reference pile,
  2026-06-20); Microsoft, Google, and the rest follow. `high`.
- **Recommendation:** use Arabic numerals everywhere in Cmdr. `Intl.NumberFormat('hi')` produces Arabic digits by
  default. Hindi (hi-IN) uses the Indian digit-grouping system (1,00,000, the lakh/crore grouping) with a period
  decimal; `formatNumber()` / `formatBytes()` produce these from the locale. Never hardcode separators or assume
  thousands-grouping.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim (Apple and
Microsoft copyrighted; GNOME GPL). Top source is macOS (Tier 1); Microsoft terminology and GNOME Nautilus cross-check.
Evidence verified against the reference pile (`_ignored/i18n/hi/`) on 2026-06-20.

- **file: `फ़ाइल`** · macOS, Microsoft. English loan in Devanagari, standard. Grammatically feminine. `high`.
- **folder: `फ़ोल्डर`** · macOS, Microsoft. English loan, standard. Masculine. `high`.
- **copy: `कॉपी करें`** (verb) · macOS. Polite imperative on buttons. `high`.
- **move: `ले जाएँ`** (UI verb) / `मूव` (loan, macOS uses both) · macOS. Prefer `ले जाएँ` for the action. `high`.
- **delete: `हटाएँ`** · macOS. Native Hindi, polite imperative. `high`.
- **open: `खोलें`** · macOS. `high`.
- **cancel: `रद्द करें`** · macOS. `high`.
- **Trash: `रद्दी`** · macOS Finder (`रद्दी ${entities}` = move to trash). `high`.
- **eject: `बाहर निकालें`** · macOS. Verify against Cmdr's eject context. `high`.
- **search: `खोज`** (noun) / `खोजें` (verb) · macOS. `high`.
- **settings: `सेटिंग`** · macOS (`Apple खाता सेटिंग`). `high`.
- **volume (disk): `वॉल्यूम`** · macOS (mounted-disk sense). `high`.
- **tab: `टैब`** · macOS. English loan, standard. `high`.
- **new folder: `नया फ़ोल्डर`** · macOS. `high`.

Pane, listing, transfer, bookmark, viewer, rename: triangulate during the first pass and record here with sources +
confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: **`one`, `other`** (verified with `new Intl.PluralRules('hi')`, 2026-06-20). Write both branches.

- **Gotcha: Hindi's `one` covers more than "exactly 1".** CLDR maps both 0 and 1 (and some fractions) to `one` for
  Hindi, so the `one` branch must read naturally for 0 too, or use an `=0` exact branch where the English needs a
  distinct zero phrasing. Don't assume `one` means only the integer 1.
- Hindi pluralizes nouns with inflection and gender/case agreement, so write the natural plural per noun (e.g. `फ़ाइल` →
  `फ़ाइलें`, `फ़ोल्डर` → `फ़ोल्डर`/`फ़ोल्डरों` depending on case) rather than appending mechanically, and keep verb
  agreement consistent inside each branch.

## Notes and decisions

- **No letter case; the sentence-case rule is moot for Devanagari.** The script is unicameral. Keep Latin brand words
  (Cmdr, macOS) as-is.
- **Quotation marks:** Hindi commonly uses the same double quotes `"…"` as English for quoted names; macOS Hindi Finder
  quotes filenames with curly quotes. Follow the English source's quote style unless it reads wrong.
- **Spaces around inserts:** a `{placeholder}` (path, filename) sits inside a Devanagari sentence; keep the surrounding
  postpositions and spacing so the sentence reads correctly regardless of the inserted value's script or length.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Hindi rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/hi/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
