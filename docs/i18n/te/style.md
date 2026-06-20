# Telugu (te) translation style guide

Working notes for translating Cmdr into Telugu. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Telugu.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them.

- **Regional target: `te-IN`, with one unified written standard, high.** Telugu is spoken across Telangana and Andhra
  Pradesh; the spoken dialects differ, but the written/formal standard used in software is largely unified, and
  Microsoft targets `te-IN`. There's no Telangana-vs-Andhra catalog split to make: ship one `te` catalog in the standard
  written register. Flagged only because region is nominally a product call; the default is low-risk.
- **Native-coinage vs English-loanword leaning, tentative.** See the tech-term decision point. Microsoft leans toward
  English loanwords for several core terms (folder, file, trash) while GNOME/Thunar use native Telugu coinages; the
  choice sets the catalog's whole register, so it's worth David's glance even though a default is recommended below.

## Voice and tone

Friendly, concise, active, calm. Microsoft's Telugu style guide sets the same register Cmdr wants: "warm and relaxed",
"crisp and clear", everyday conversational Telugu over formal/technical Telugu (verified against the reference pile,
`te/microsoft-style-guides/StyleGuide.pdf`, 2026-06-20). Keep sentences short and spoken; avoid the heavy Sanskritized
literary register (grandhika), use the modern standard (vyavaharika).

Error messages stay calm and actionable. Don't reach for a "లోపం" (error) or "విఫలమైంది" (failed) label; phrase the
problem and the next step, the way the English voice avoids "error" / "failed".

## Formality

- **Address the user as `మీరు` (polite/plural), high.** Telugu has a T-V split: `నువ్వు` (familiar singular) vs `మీరు`
  (polite/plural). Microsoft's Telugu style guide uses `మీరు` to politely address the user (verified against the
  reference pile, 2026-06-20). Never use `నువ్వు` in UI: it reads as overly familiar or talking down. Keep verb endings
  (`-ండి` polite imperatives) and pronouns in the `మీరు` register consistently.
- **Buttons and menu items: polite imperative.** Telugu UI uses the polite `-ండి` imperative for actions: "రద్దు చేయి" /
  "రద్దుచేయి" (cancel), "తొలగించు" (delete), "నకలు చెయ్యి" (copy), "తరలించు" (move), "తెరువు" (open), "మూసివేయి"
  (close). Keep these concise; don't expand to full polite request sentences on buttons.

## Decision points

### Script: Telugu script (Brahmic abugida), confirm native, never Latin

- Telugu is written in the **Telugu script**, a Brahmic abugida (NOT Latin, NOT Devanagari, and distinct from Tamil's
  script). Every major that localizes Telugu ships native Telugu script, never romanization: Microsoft, Google (Android,
  Search, Translate UI), and Spotify all render real Telugu. `high`.
- **Rendering care:** Telugu is a complex script with stacked consonant conjuncts (the secondary form below the base)
  and combining vowel signs (matras); the rendered glyph cluster spans multiple code points and isn't left-to-right
  code-point order. Don't measure, truncate, or reverse strings by code unit, and confirm the font stack shapes Telugu
  conjuncts correctly (no broken clusters or tofu). This is a real complex-script risk Latin languages don't have.
- **Do NOT transliterate** product UI into Latin; romanized Telugu is an informal chat register, never a localized-UI
  convention.

### Major-vendor coverage: low for macOS (lean on Microsoft + Google + GNOME/Xfce)

- **Apple does NOT localize macOS into Telugu.** macOS ships Telugu keyboards (InScript, QWERTY, transliteration) but no
  Telugu system display language, so there's no Tier-1 macOS Finder evidence for Telugu terms. The reference pile
  confirms it: `te/` has `gnome-nautilus`, `microsoft-style-guides`, `microsoft-terminology`, and `xfce-thunar`, but no
  `macOS/` (verified against the reference pile, 2026-06-20). Telugu actually has the widest pile of the three (it's the
  only one with Thunar).
- **Who does localize:** Microsoft (Windows, Office; terminology authority), Google (Android, Search, Workspace UI),
  GNOME Nautilus + Xfce Thunar (file-manager-domain cross-check, and they often agree with each other against
  Microsoft), and Spotify (mobile). With no macOS tier, the tiebreak is "Microsoft for product consistency, GNOME/Thunar
  as the file-manager-native register".

### Tech-term strategy: weigh Microsoft loanwords vs GNOME/Thunar native coinages

- The genuine split is naturalized English loanword (Microsoft) vs native Telugu coinage (GNOME/Thunar), and they
  disagree on the most visible words (verified against the reference pile, 2026-06-20):
  - folder → Microsoft `ఫోల్డర్` (loanword) vs GNOME `సంచయం` (native).
  - file → Microsoft `ఫైల్` (loanword) vs Thunar `దస్త్రం` (native).
  - trash → Microsoft `రీసైకిల్ బిన్` (loanword) vs GNOME/Thunar `చెత్తబుట్ట` (native "rubbish bin"). GNOME and Thunar
    agree here, strengthening the native form.
  - They agree on copy → `నకలు చెయ్యి`, move → `తరలించు`, delete → `తొలగించు`, settings → `సెట్టింగ్‌లు`.
- **Recommendation:** lean toward the native GNOME/Thunar coinages for the file-manager-core nouns where two independent
  open-source catalogs agree (`సంచయం`, `చెత్తబుట్ట`, `దస్త్రం`), because Cmdr is a file manager and that's the register
  users meet in Nautilus/Thunar; the Microsoft loanwords are widely understood and a valid alternative if David prefers
  them for consistency with Windows. This is the one term-register call worth settling up front. `tentative` (real
  source split, flagged above); the agreed terms are `high`.

### Gender: grammatical gender in 3rd person, keep UI gender-neutral

- Telugu marks gender in the third person and in verb agreement (he/she/it distinctions). Cmdr's strings address the
  user in second person (`మీరు`, ungendered) and refer to files/items in the neuter, so most UI sidesteps gender
  naturally. `high`.
- Watch any string that would refer to a person in third person; keep it second-person or neuter so the catalog never
  has to guess a user's gender.

### Length: overflow-check

- Telugu strings can run longer than English, and stacked conjuncts plus matras raise line-height demands.
  Overflow-check buttons and menus against the pseudolocale (`en-XA`) and real Telugu strings, and confirm line height
  doesn't clip the tallest stacked glyph clusters. `high`.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim (Microsoft
copyrighted; GNOME/Thunar GPL). Top source is Microsoft terminology (no macOS tier); GNOME Nautilus and Xfce Thunar are
the file-manager cross-check. Evidence verified against the reference pile (`_ignored/i18n/te/`) on 2026-06-20.

- **file: `దస్త్రం`** · Thunar (native) / Microsoft `ఫైల్` (loanword). Prefer `దస్త్రం` for the file-manager-native
  register, or `ఫైల్` for Microsoft consistency; pick one. `tentative` (real split).
- **folder: `సంచయం`** · GNOME (native) / Microsoft `ఫోల్డర్` (loanword). Prefer `సంచయం`, or `ఫోల్డర్` for Microsoft
  consistency. `tentative` (real split).
- **directory: `సంచయని`** · Microsoft terminology. Use only where the technical directory sense matters, else folder
  term. `high`.
- **copy: `నకలు చెయ్యి`** · Microsoft terminology. Imperative. `high`.
- **move: `తరలించు`** · Microsoft terminology. `high`.
- **delete: `తొలగించు`** · Microsoft terminology. `high`.
- **cancel: `రద్దుచేయి`** · GNOME (`రద్దుచేయి`), Microsoft (`రద్దు చేయి`) agree. `high`.
- **search: `వెతుకు` (verb) / `శోధన` (noun)** · GNOME (`వెతుకు`), Microsoft (`శోధన`). The button is the imperative
  `వెతుకు`. `high`.
- **settings: `సెట్టింగ్‌లు`** · Microsoft terminology. `high`.
- **trash: `చెత్తబుట్ట`** · GNOME and Thunar agree (native); Microsoft uses `రీసైకిల్ బిన్`. Prefer `చెత్తబుట్ట` (two
  file-manager sources agree). `high` for the GNOME/Thunar choice.
- **drive: `డ్రైవ్`** · Microsoft terminology (naturalized loan). `high`.
- **volume: `వాల్యూమ్` / disk-volume sense** · Microsoft maps "volume" → `పరిమాణం` (the size/quantity sense, wrong for a
  disk); use the disk-volume reading. `tentative`, confirm the disk-volume term against Google/Android Telugu.
- **bookmark: `బుక్‌మార్క్`** · Microsoft terminology (naturalized loan, verb `బుక్‌మార్క్ చేయి`). `high`.
- **open: `తెరువు` / close: `మూసివేయి`** · Microsoft terminology. `high`.

Add terms as they come up, in this same `chosen · sources · confidence` shape.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('te')`, 2026-06-20). Write both branches.

- Telugu pluralizes with the `-లు` suffix (ఫైల్ → ఫైళ్లు; సంచయం → సంచయాలు), with stem changes/sandhi, so write the
  natural plural for each noun rather than appending mechanically.
- Keep verb and pronoun agreement consistent with the counted noun inside each branch.

## Notes and decisions

- **No title case; Telugu has no letter case.** The script is unicameral, so the app's sentence-case rule is moot for
  Telugu text; just keep Latin brand words (Cmdr, macOS) as-is.
- **Numbers and dates come from the formatter layer.** Telugu (te-IN) uses the Indian digit-grouping system (1,00,000,
  the lakh/crore grouping) and a period decimal; `formatNumber()` / `formatBytes()` produce these from the locale. Never
  hardcode separators or assume thousands-grouping.
- **Quotation marks:** standard double quotes "…"; no special pair needed.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/te/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
