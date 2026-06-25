# Swahili (sw) translation style guide

Working notes for translating Cmdr into Swahili. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Swahili.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them.

- **Regional target: `sw-TZ` (Tanzanian Kiswahili sanifu), high.** Standard Swahili (Kiswahili sanifu) is codified
  around the Tanzanian norm, and that's the variant Microsoft's style guide targets (`swa-tza-styleguide.pdf`). It reads
  as neutral, educated Swahili across Kenya, Tanzania, Uganda, and the DRC, so it's the safe single target. Flagged
  because picking a region is a product call, but the default is low-risk: ship one `sw` catalog written in sanifu, not
  separate `sw-KE` / `sw-TZ` catalogs.
- **Native-term leaning over English loanwords, tentative.** See the tech-term decision point. The leaning (prefer the
  Microsoft-established Swahili coinage where one exists, borrow only when no settled native term does) is a judgment
  call worth David's glance, because Swahili desktop UI is low-resource and some coinages ("kabrasha" for folder) read
  less familiar to users than the English word they see elsewhere.

## Voice and tone

Friendly, concise, active, calm. Swahili carries Cmdr's English voice well: it has a rich, direct imperative mood that's
the natural register for UI actions, and no formal/informal second-person split to navigate (see Formality). Keep
sentences short and spoken, not bureaucratic.

Error messages stay calm and actionable. Swahili has no need to lean on a "kosa" (error) or "imeshindwa" (failed) label:
phrase the problem and the next step ("Haikuwezekana kubadilisha jina la faili. Jaribu tena?"), the same way the English
voice avoids "error" / "failed".

## Formality

- **No T-V distinction to resolve.** Swahili second person is `wewe` (singular) / `nyinyi` (plural); there's no
  polite-vs-familiar honorific axis like Tamil or Telugu, so the gendered/formality pitfall that dogs Indic and European
  UI doesn't exist here. Address the user directly and naturally.
- **Buttons and menu items: imperative verb.** Swahili imperatives are short and idiomatic for UI: "Hifadhi" (save),
  "Ghairi" (cancel), "Futa" (delete), "Nakili" (copy), "Hamisha" (move), "Fungua" (open), "Funga" (close). The
  imperative singular (addressing the user as one person) is the UI norm; don't use the polite subjunctive ("uhifadhi")
  on buttons, it reads as a request, not an action.

## Decision points

### Script: Latin (no decision)

- Modern Swahili is written in **Latin script**, universally, in every OS, browser, and app. The historical
  Arabic-script tradition (Ajami) is scholarly/marginal and never used in software. No transliteration question, no
  complex-script rendering risk. `high`.
- Practical effect: Swahili needs none of the Brahmic-shaping care that Tamil and Telugu do. Treat layout and font
  exactly like a European Latin language.

### Major-vendor coverage: low (lean on Microsoft + Google)

- **Apple does NOT localize macOS into Swahili.** macOS ships a Swahili keyboard but no Swahili system display language,
  so there's no Tier-1 macOS Finder evidence for Swahili terms (unlike the European languages in this pile). The
  reference pile confirms this: `sw/` has only `microsoft-terminology`, no `macOS/`, no GNOME, no Thunar (verified
  against the reference pile, 2026-06-20).
- **Who does localize:** Microsoft (Office, Windows partial; the authority here), Google (Translate, Android, Search
  UI), and Spotify (mobile app). Use Microsoft terminology as the spine, Google's Android/Material Swahili as a
  cross-check for any term Microsoft lacks. There's no macOS-vs-Windows split to adjudicate, so the usual "macOS wins"
  tiebreak doesn't apply; Microsoft is effectively the top tier for Swahili.

### Tech-term strategy: prefer the settled native coinage, borrow sparingly

- The real split for Swahili is native coinage vs English loanword, and Microsoft has already coined native terms for
  most file-manager vocabulary (verified against the reference pile, 2026-06-20): folder → `kabrasha`, file → `faili`
  (this one is a naturalized loan), directory → `saraka`, search → `utafutaji`, copy → `nakili`, move → `hamisha`, trash
  → `kijalala`, drive → `kiendeshi`, server → `seva`, settings → `mipangilio`.
- **Recommendation:** prefer the Microsoft Swahili term where it exists; borrow the English word only when no settled
  native term does (rare for file-manager basics). "faili" and "seva" are accepted naturalized loans; "kabrasha" /
  "saraka" / "kijalala" are native coinages worth keeping for consistency even though some users also recognize the
  English word. Don't mix both in the same catalog. `tentative` as a blanket policy (David's call, flagged above), but
  per-term the Microsoft choice is `high`.

### Gender: noun classes, no masculine/feminine (no decision)

- Swahili has no grammatical gender; it has noun classes (the Bantu m-/wa-, ki-/vi-, etc. system) that drive agreement,
  but nothing maps to "he/she" the way Tamil/Telugu third person or Romance gender does. The gendered-UI pitfall (a
  string that has to guess the user's or a referent's gender) simply doesn't arise. `high`.
- What does need care: noun-class agreement on adjectives, possessives, and verb prefixes inside a counted/possessed
  phrase (see Plurals). That's grammar to get right, not a product decision.

### Length: overflow-check

- Swahili runs noticeably longer than English (agglutinative verbs, longer words: "Move to trash" → "Hamishia kwenye
  kijalala"). Expect 20-40% expansion on action labels. Overflow-check every button and menu against the pseudolocale
  (`en-XA`) and the longest real Swahili strings. `high`.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim (Microsoft
strings are copyrighted). For Swahili the top source is Microsoft terminology (no macOS tier exists). Evidence verified
against the reference pile (`_ignored/i18n/sw/`) on 2026-06-20.

- **file: `faili`** · Microsoft terminology. Naturalized loan, universally understood. `high`.
- **folder: `kabrasha`** · Microsoft terminology. Native coinage; keep it consistent rather than borrowing "folda".
  `high`.
- **directory: `saraka`** · Microsoft terminology (the filesystem-directory sense; Microsoft also lists a descriptive
  "mpangilio orodha", but `saraka` is the term to use). Use only where the technical directory sense matters, else
  `kabrasha`. `high`.
- **copy: `nakili`** · Microsoft terminology. Imperative on buttons. `high`.
- **move: `hamisha`** · Microsoft terminology. `high`.
- **delete: `futa`** · Microsoft terminology. Imperative. `high`.
- **cancel: `ghairi`** · standard Swahili UI imperative for cancel (Microsoft "Katisha" leans toward interrupt/cut-off;
  `ghairi` is the cleaner "cancel this action"). `tentative` (sources lean differently), confirm against Google Android
  Swahili if available.
- **search: `utafutaji` (noun) / `tafuta` (verb)** · Microsoft terminology (`utafutaji`). The action button is the
  imperative `tafuta`. `high`.
- **settings: `mipangilio`** · Microsoft terminology. `high`.
- **trash: `kijalala`** · Microsoft terminology (also the "Recycle Bin" target). The move-to-trash action is "Hamishia
  kwenye kijalala". `high`.
- **drive: `kiendeshi`** · Microsoft terminology. Physical/removable drive. `high`.
- **volume: `juzuu`** · Microsoft terminology, the disk-volume sense. Note: Microsoft also maps "volume" → `sauti`, but
  that's the audio-loudness sense; use `juzuu` for a mounted disk volume, never `sauti`. `high`.
- **server: `seva`** · Microsoft terminology. Naturalized loan. `high`.
- **bookmark: `alamisho` (noun) / `alamisha` (verb)** · Microsoft terminology (`alamisha` verb). `high`.
- **tab: `kichupo`** · Microsoft terminology. `high`.
- **pane: `kidirisha`** · Microsoft terminology (literally a small window/region). The two file lists. `tentative` (no
  file-manager source confirms the pane sense specifically).
- **open: `fungua` / close: `funga`** · Microsoft terminology. `high`.

Add terms as they come up, in this same `chosen · sources · confidence` shape.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('sw')`, 2026-06-20). Write both branches.

- Swahili pluralizes by noun-class prefix, not a suffix: "faili" is often invariant, but many nouns change (mtu/watu,
  kifaa/vifaa). Write the natural plural for each counted noun; don't pattern-match off English "+s".
- The count interacts with verb and adjective agreement via the noun class ("faili 1 imechaguliwa" vs "faili 5
  zimechaguliwa"): keep the agreement prefix correct inside each branch, not just the number.

## Notes and decisions

- **Sentence case is native.** Swahili capitalizes only the first word and proper nouns; days and months are lowercase.
  The app's sentence-case rule applies cleanly. Don't title-case.
- **Numbers and dates come from the formatter layer.** Swahili uses a period decimal separator and a non-breaking space
  for thousands groups; `formatNumber()` / `formatBytes()` produce these from the locale. Never hardcode separators.
- **Time-of-day phrasing is an EAT trap, not a formatter one.** Traditional Swahili clock time counts from dawn (saa
  moja = 7 a.m.), six hours offset from the Western clock. Software universally uses the Western am/pm reckoning anyway,
  so if any string ever phrases a time in words ("at 3 o'clock"), keep it on the Western clock the formatter produces;
  don't "correct" it to traditional reckoning. (Numeric clock output from the formatter is already fine.)
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/sw/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
