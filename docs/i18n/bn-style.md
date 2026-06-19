# Bengali (bn) translation style guide

Working notes for translating Cmdr into Bengali. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Bengali.

Bengali coverage is mixed: there's NO macOS Bengali display language (no Tier-1 Apple evidence), so the top sources are
Microsoft (terminology + style guide) and GNOME Nautilus. Sources mined for this guide: the Microsoft Bengali (BANGLA)
terminology, the bn-IN Microsoft style guide, and the GNOME Nautilus Bengali catalog.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## Decisions to confirm with David

These are calls a translator can't make alone. The rest of this guide assumes them.

- **Regional target: India (`bn-IN`) vs Bangladesh (`bn-BD`), high.** Bengali localization splits into bn-IN (India,
  ~West Bengal) and bn-BD (Bangladesh). The written standard is largely shared, but vocabulary, some loanword choices,
  and digit-grouping/date conventions differ, and the two communities are large and distinct. The reference pile carries
  bn-IN (Microsoft style guide) and the base `bn`; ship one `bn` catalog to a chosen norm. Recommendation: target the
  shared standard literary Bengali (sadhu-free, cholito modern), leaning bn-IN where the pile gives evidence, since
  that's what the sources cover; flag bn-BD as a possible later override. Picking the primary region is a product call.
- **Native-coinage vs English/transliterated loanword leaning, tentative.** Microsoft and GNOME diverge on visible terms
  (trash: Microsoft transliterates `রিসাইকেল বিন` "recycle bin"; GNOME uses native `আবর্জনার বাক্স` "rubbish box").
  The choice sets the catalog's register; recommendation leans native GNOME forms (see the tech-term decision point),
  worth David's glance.

## Voice and tone

Cmdr's Bengali voice is friendly, concise, active, and never alarmist, matching the English. Microsoft's Bengali (bn-IN)
voice guidance lines up with Cmdr's: "warm and relaxed, less formal," everyday words over stiff formal vocabulary,
writing for scanning first (verified against the reference pile, `bn-IN/microsoft-style-guides/StyleGuide.pdf`,
2026-06-20). Carry that over: use the modern cholito-bhasha (spoken-standard) register, NOT the archaic sadhu-bhasha
literary register, which reads as old-fashioned.

Error and warning messages stay calm and actionable. Keep the English rule of avoiding the words "error" and "failed";
phrase what happened and the next step (Bengali has neutral framings around `করা যায়নি` "couldn't…") rather than a loud
`ত্রুটি` (error) / `ব্যর্থ` (failed) label.

## Formality

- **Address the user as `আপনি` (respectful you), high.** Bengali has a three-way politeness split: `তুই`
  (intimate/rude in UI), `তুমি` (familiar), and `আপনি` (respectful/polite). The GNOME Bengali catalog uses `আপনি`
  exclusively (38 instances, zero `তুমি`/`তুই`, verified against the reference pile, 2026-06-20), and the bn-IN Microsoft
  style guide addresses the user politely with `আপনি`. Always use `আপনি`; never `তুই`/`তুমি` in UI. Keep verb endings in
  the `আপনি` register (the polite `-উন`/`-ন` imperative) consistently.
- **Buttons and menu items: polite imperative.** GNOME labels actions in the polite imperative: `মুছে ফেলুন` (delete),
  `খুলুন` (open), `বাতিল` (cancel), `বের করে আনা` (eject). Keep these concise polite-imperative forms; don't expand to
  full request sentences on buttons.

## Decision points

### Script: Bengali script (Brahmic abugida), confirm native, never Latin

- Bengali is written in the **Bengali-Assamese script**, a Brahmic abugida (NOT Latin, NOT Devanagari). Every major that
  localizes Bengali ships native Bengali script: Microsoft, Google (Android, Search), and the GNOME catalog all render
  real Bengali (Microsoft/GNOME verified against the pile; Google web-evidenced, unverified). No Apple/macOS Bengali
  exists to cross-check. `high`.
- **Rendering care:** Bengali uses combining vowel signs, the hasanta (virama), and heavy consonant-conjunct
  ligatures (juktakkhor) where two or more consonants fuse into one glyph (e.g. ক্ষ, ঞ্জ); some vowel signs render
  before/around their base, so visual order isn't code-point order. Don't measure, truncate, or reverse strings by code
  unit, and confirm the app's font stack shapes Bengali correctly (conjuncts form, vowel signs attach, no tofu boxes).
  This is a real complex-script rendering risk, and conjunct-heavy fonts vary in quality.
- **Do NOT transliterate** product UI into Latin "Banglish"; that's an informal chat register, never a localized-UI
  convention.

### Major-vendor coverage: NO macOS tier, Microsoft + GNOME are the authorities

- **Apple does NOT localize macOS into Bengali.** There's no Tier-1 macOS Finder evidence for Bengali terms; the
  reference pile confirms `bn/` has `gnome-nautilus` and `microsoft-terminology` but no `macOS/` (verified against the
  pile, 2026-06-20). So the usual "macOS wins" tiebreak is replaced by "Microsoft and GNOME, with GNOME as the
  file-manager-specific authority."
- Microsoft (Windows, Office; the terminology source) and GNOME Nautilus (the file-manager-domain cross-check) are the
  two pillars. When they disagree (they do on trash), weigh GNOME for file-manager-native register and Microsoft for
  broad-product familiarity, and record the split.

### Tech-term strategy: prefer the established native Bengali term, borrow sparingly

- The genuine split is native Bengali term vs transliterated English loanword, and the two sources disagree on visible
  words (verified against the reference pile, 2026-06-20):
  - trash → Microsoft `রিসাইকেল বিন` (transliterated "recycle bin") vs GNOME `আবর্জনার বাক্স` (native "rubbish box").
  - They agree on file → `ফাইল` and folder → `ফোল্ডার` (both naturalized loans, universally understood).
- **Recommendation:** prefer the established native Bengali term where one is natural and common (Bengali has strong
  native vocabulary), and accept the naturalized loan where that's genuinely what users say (`ফাইল`, `ফোল্ডার`). For the
  trash split, lean GNOME's native `আবর্জনা…` form for a file-manager-native, less-corporate register; Microsoft's
  transliteration is the alternative if David prefers Windows-familiar wording. `tentative` on the trash split (flagged
  above); the agreed terms are `high`.
- **Watch Microsoft's wrong-sense terms:** Microsoft's BANGLA terminology maps "volume" → `স্বর প্রাবল্য`, which is
  AUDIO loudness, not a mounted disk volume. Don't use it for Cmdr's disk-volume sense; pick a disk-volume term during
  the first pass and record it.

### Gender: grammatically light; keep UI gender-neutral

- Bengali is unusually gender-light for an Indic language: it has NO grammatical gender on nouns and NO gender agreement
  on verbs or adjectives (unlike Hindi). Third-person pronouns don't distinguish he/she. So Bengali UI is inherently
  gender-neutral as long as strings address the user with `আপনি` and refer to files/items as things. `high`. No special
  gender handling needed.

### Numerals: Arabic (0-9), not Bengali (০-৯)

- Bengali has its own digits (`০১২৩৪৫৬৭৮৯`) but modern Bengali software predominantly uses Arabic numerals (0-9) for
  counts, sizes, and percentages; locale defaults in ICU/`Intl` return Western digits for both bn-IN and bn-BD
  (web-evidenced; users have even requested native-numeral support, confirming the default is Arabic). `high`.
- **Recommendation:** use Arabic numerals everywhere in Cmdr; `Intl.NumberFormat('bn')` produces Western digits by
  default. Bengali uses the Indian digit-grouping system (10,00,000, the lakh/crore grouping) with a period decimal;
  `formatNumber()` / `formatBytes()` produce these from the locale. Never hardcode separators or assume thousands-
  grouping.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim (Microsoft
copyrighted; GNOME GPL). NO macOS tier for Bengali; top sources are Microsoft terminology and GNOME Nautilus (GNOME is
the file-manager authority). Evidence verified against the reference pile (`_ignored/i18n/bn/`) on 2026-06-20.

- **file: `ফাইল`** · Microsoft, GNOME agree. Naturalized loan, standard. `high`.
- **folder: `ফোল্ডার`** · Microsoft, GNOME agree. Naturalized loan, standard. `high`.
- **copy: `অনুলিপি করুন`** (verb) · Microsoft (`অনুলিপি করা`), GNOME uses `অনুলিপি`. Polite imperative on buttons.
  `high`.
- **move: `সরান`** / `স্থানান্তর করুন` · GNOME (`স্থানান্তর` = transfer/move). `high`.
- **delete: `মুছে ফেলুন`** · GNOME. Native Bengali, polite imperative. `high`.
- **open: `খুলুন`** · GNOME. `high`.
- **cancel: `বাতিল`** · GNOME, Microsoft. `high`.
- **Trash: `আবর্জনার বাক্স`** · GNOME (native "rubbish box"); Microsoft's `রিসাইকেল বিন` (transliterated) is the
  alternative. Pick one, keep consistent. `tentative` (real source split, flagged above).
- **eject: `বের করে আনা`** · GNOME. Verify against Cmdr's eject context. `high`.
- **search: `অনুসন্ধান`** · GNOME, Microsoft (`সন্ধান`). `high`.
- **settings: `সেটিংস`** · Microsoft. `high`.
- **volume (disk): pick during first pass** · NOT Microsoft's `স্বর প্রাবল্য` (that's audio loudness). Triangulate a
  mounted-disk term (e.g. `ভলিউম` loan) and record. `tentative`.
- **tab: `ট্যাব`** · Microsoft. Naturalized loan. `high`.

Pane, listing, transfer, bookmark, viewer, rename, new folder: triangulate during the first pass and record here with
sources + confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: **`one`, `other`** (verified with `new Intl.PluralRules('bn')`, 2026-06-20). Write both branches.

- **Gotcha: Bengali's `one` covers more than "exactly 1".** CLDR maps 0 and 1 (and some fractions) to `one` for Bengali,
  so the `one` branch must read naturally for 0 too, or use an `=0` exact branch where the English needs a distinct zero
  phrasing. Don't assume `one` means only the integer 1.
- Bengali pluralizes with classifiers/quantifiers and optional plural markers (`-গুলি`/`-গুলো`/`-রা`), and counted
  nouns usually take a classifier (`টি`/`টা`/`জন`): a natural counted string is `{count}টি ফাইল` "{count} files" rather
  than pluralizing the noun. Mind the classifier per noun, and keep agreement consistent inside each branch. Because
  Bengali has no gender agreement, plural branches are simpler than Hindi's.

## Notes and decisions

- **No letter case; the sentence-case rule is moot for the Bengali script.** It's unicameral. Keep Latin brand words
  (Cmdr, macOS) as-is.
- **Punctuation:** Bengali uses the daṛi `।` as its full stop (sentence-final), not a Latin period, in running Bengali
  prose; use `।` to end Bengali sentences. Commas, colons, and question marks follow Latin forms. Quotation marks use
  `"…"`.
- **Spaces around inserts:** a `{placeholder}` (path, filename) sits inside a Bengali sentence; keep the surrounding
  postpositions and spacing so the sentence reads correctly regardless of the inserted value's script or length.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Bengali rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
