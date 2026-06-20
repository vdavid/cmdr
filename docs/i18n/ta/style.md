# Tamil (ta) translation style guide

Working notes for translating Cmdr into Tamil. Read [`README.md`](../README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into Tamil.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them.

- **Regional target: `ta-IN` (India), high.** Tamil localization splits into `ta-IN` (India) and `ta-LK` (Sri Lanka);
  Microsoft targets `ta-IN` and so does the bulk of desktop UI work. The written standard differs mainly in some
  vocabulary and idiom, not script. Ship one `ta` catalog written to the `ta-IN` norm. Flagged only because choosing a
  region is a product call; the default is low-risk.
- **Native-coinage vs English-loanword leaning, tentative.** See the tech-term decision point. Microsoft and GNOME
  diverge on several core file-manager terms (folder, trash), and the choice sets the whole catalog's register, so it's
  worth David's glance even though a default is recommended below.

## Voice and tone

Friendly, concise, active, calm. Microsoft's Tamil style guide explicitly sets the same register Cmdr wants: "warm and
relaxed", "crisp and clear", everyday conversational Tamil over formal/technical Tamil (verified against the reference
pile, `ta/microsoft-style-guides/StyleGuide.pdf`, 2026-06-20). That voice carries Cmdr's English over cleanly: keep
sentences short and spoken, avoid the heavy Sanskritized register.

Error messages stay calm and actionable. Don't reach for a "பிழை" (error) or "தோல்வி" (failure) label; phrase the
problem and the next step, the way the English voice avoids "error" / "failed".

## Formality

- **Address the user as `நீங்கள்` (polite/plural), high.** Tamil has a strong T-V split: `நீ` (familiar singular) vs
  `நீங்கள்` (polite/plural). Microsoft's Tamil style guide states outright to address the user as `நீங்கள்`, directly or
  indirectly (verified against the reference pile, 2026-06-20). Never use `நீ` in UI: it reads as curt or talking down.
  Keep verb endings and pronouns in the `நீங்கள்` register consistently.
- **Buttons and menu items: imperative in the polite register.** Tamil UI uses the polite imperative for actions:
  "சேமி" (save), "ரத்து செய்" (cancel), "நீக்கு" (delete), "நகலெடு" (copy), "நகர்த்து" (move), "திற" (open), "மூடு"
  (close). These short imperative forms are the established UI norm; keep them concise, don't expand to full polite
  request forms on buttons.

## Decision points

### Script: Tamil script (Brahmic abugida), confirm native, never Latin

- Tamil is written in the **Tamil script**, a Brahmic abugida (NOT Latin, NOT Devanagari). Every major that localizes
  Tamil ships native Tamil script, never romanized transliteration: Microsoft, Google (Android, Search, Translate UI),
  and Spotify all render real Tamil. `high`.
- **Rendering care:** Tamil uses combining vowel signs and the pulli (virama); some vowel signs render to the left of
  their consonant (கொ, கோ) so the visual order isn't the code-point order. Don't measure, truncate, or reverse strings
  by code unit, and confirm the app's font stack actually shapes Tamil (combining marks attach correctly, no tofu
  boxes). This is a real complex-script rendering risk that Latin languages don't have.
- **Do NOT transliterate** product UI into Latin "Tanglish"; that's an informal chat register, never a localized-UI
  convention.

### Major-vendor coverage: low for macOS (lean on Microsoft + Google + GNOME)

- **Apple does NOT localize macOS into Tamil.** macOS ships Tamil keyboards (Anjal, Tamil 99, transliteration) but no
  Tamil system display language, so there's no Tier-1 macOS Finder evidence for Tamil terms. The reference pile confirms
  it: `ta/` has `gnome-nautilus`, `microsoft-style-guides`, and `microsoft-terminology`, but no `macOS/` (verified
  against the reference pile, 2026-06-20).
- **Who does localize:** Microsoft (Windows, Office; the terminology authority here), Google (Android, Search, Workspace
  UI), GNOME Nautilus (the file-manager-domain cross-check), and Spotify (mobile). Because there's no macOS tier, the
  usual "macOS wins" tiebreak is replaced by "Microsoft wins, GNOME as the file-manager-specific sanity check".

### Tech-term strategy: prefer the established Tamil term, borrow sparingly

- The genuine split is native Tamil term vs naturalized English loanword, and the two top sources disagree on the most
  visible words (verified against the reference pile, 2026-06-20):
  - folder → Microsoft `கோப்புறை` vs GNOME Nautilus `அடைவு`.
  - trash → Microsoft `மறுசுழற்சிக் கூடை` (recycle-bin flavored) vs GNOME `குப்பைதொட்டி` (literally "rubbish bin").
  - They agree on file → `கோப்பு`, delete → `நீக்கு`, copy → `நகலெடு`, move → `நகர்த்து`, settings → `அமைப்புகள்`.
- **Recommendation:** prefer the established native Tamil term over an English loan (Tamil has strong, widely-understood
  native IT vocabulary, unlike lower-resource languages). For the folder/trash split, lean Microsoft for product-term
  consistency (`கோப்புறை`, and a trash term, see glossary) since Cmdr has no macOS Tamil to defer to, but the GNOME
  forms are the file-manager-native alternatives if David prefers a plainer register. `tentative` on the split (flagged
  above); the agreed terms are `high`.

### Gender: grammatical gender in 3rd person, keep UI gender-neutral

- Tamil marks gender in the third person (அவன் he / அவள் she / அது it, and gendered verb agreement). Cmdr's strings
  address the user in second person (`நீங்கள்`, ungendered) and refer to files/items in the neuter/non-rational class
  (`அது` / plural `அவை`), so most UI sidesteps gender naturally. `high`.
- Watch any string that would otherwise refer to a person in third person; keep it second-person or neuter so the
  catalog never has to guess a user's gender.

### Length: overflow-check

- Tamil strings can run longer than English and the script's combining marks add vertical/line-height demands. Action
  labels often expand ("Move to trash" → a multi-word phrase). Overflow-check buttons and menus against the pseudolocale
  (`en-XA`) and real Tamil strings, and confirm line height doesn't clip the tallest stacked glyphs. `high`.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim (Microsoft
copyrighted; GNOME is GPL). Top source for Tamil is Microsoft terminology (no macOS tier); GNOME Nautilus is the
file-manager cross-check. Evidence verified against the reference pile (`_ignored/i18n/ta/`) on 2026-06-20.

- **file: `கோப்பு`** · Microsoft terminology, GNOME agree. `high`.
- **folder: `கோப்புறை`** · Microsoft terminology (GNOME uses `அடைவு`; prefer the Microsoft form for product
  consistency). `high` for the Microsoft choice; the split itself is flagged above.
- **directory: `கோப்பகம்`** · Microsoft terminology. Use only where the technical directory sense matters, else
  `கோப்புறை`. `high`.
- **copy: `நகலெடு`** · Microsoft terminology. Imperative on buttons. `high`.
- **move: `நகர்த்து`** · Microsoft terminology. `high`.
- **delete: `நீக்கு`** · Microsoft terminology. `high`.
- **cancel: `ரத்து செய்`** · Microsoft terminology (`ரத்து`). `high`.
- **search: `தேடு` (verb) / `தேடல்` (noun)** · Microsoft terminology. `high`.
- **settings: `அமைப்புகள்`** · Microsoft terminology. `high`.
- **trash: `மறுசுழற்சிக் கூடை`** · Microsoft terminology (recycle-bin sense). GNOME's plainer `குப்பைதொட்டி` is the
  file-manager-native alternative; pick one and keep it consistent. `tentative` (real source split).
- **drive: `இயக்ககம்`** · Microsoft terminology. `high`.
- **volume: `தொகுதி`** · Microsoft terminology, the disk-volume sense. Note Microsoft also maps "volume" → `ஒலியளவு`,
  but that's audio loudness; use `தொகுதி` for a mounted disk volume. `high`.
- **bookmark: `அடையாளக்குறி` (noun) / `அடையாளக்குறியிடு` (verb)** · Microsoft terminology. `high`.
- **open: `திற` / close: `மூடு`** · Microsoft terminology. `high`.

Add terms as they come up, in this same `chosen · sources · confidence` shape.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('ta')`, 2026-06-20). Write both branches.

- Tamil pluralizes with the `-கள்` suffix (கோப்பு → கோப்புகள்), but the suffix triggers sandhi and form changes, so
  write the natural plural for each noun rather than appending mechanically.
- The non-rational (neuter) class covers files/folders/items; keep verb and pronoun agreement in that class inside each
  branch (`அது` singular / `அவை` plural).

## Notes and decisions

- **No title case; Tamil has no letter case at all.** The script is unicameral, so the app's sentence-case rule is moot
  for Tamil text itself; just keep Latin brand words (Cmdr, macOS) as-is.
- **Numbers and dates come from the formatter layer.** Tamil (ta-IN) uses the Indian digit-grouping system
  (1,00,000, the lakh/crore grouping) and a period decimal; `formatNumber()` / `formatBytes()` produce these from the
  locale. Never hardcode separators or assume thousands-grouping.
- **Quotation marks:** Tamil uses standard double quotes "…"; no special pair needed.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/ta/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
