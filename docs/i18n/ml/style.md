# Malayalam (ml) translation style guide

Working notes for translating Cmdr into Malayalam. Read `../README.md` for how this fits the translation process. `ml`
is the base tag (the universal Malayalam set); `ml-IN` would only exist for region-specific overrides, and none are
needed today (Kerala usage is the de-facto standard and what `ml` targets).

## Voice and tone

Friendly, concise, calm, active. Same register Cmdr uses in English: a helpful peer, never bureaucratic or alarmist.
Error and warning copy stays calm and actionable; never use a Malayalam equivalent of "error" or "failed" as a scare
word. Prefer plain everyday Malayalam (the register of a well-made consumer app), not literary/Sanskritized
(`ഗ്രന്ഥഭാഷ`) prose. Verb-final word order is natural; let the action verb close the sentence rather than forcing
English order.

## Formality

Malayalam marks respect through the second-person PRONOUN, not the verb: `നീ` (informal), `നിങ്ങൾ` (polite/plural),
`താങ്കൾ` (more formal). Crucially, all three take the same verb form, so the politeness choice rarely surfaces in UI
copy.

- **Address the user as `നിങ്ങൾ`** when a pronoun is unavoidable (polite, neutral, the register Google and Microsoft use
  for Malayalam). Never `നീ`. Avoid `താങ്കൾ` (reads stiff/old-fashioned for an app).
- **Prefer pronoun-free phrasing.** Most UI copy needs no "you" at all; Malayalam drops the subject readily. "Are you
  sure you want to delete?" is best rendered without a pronoun.
- **Imperatives for actions (buttons, menu items): use the `-uക` infinitive/imperative stem** (e.g. `പകർത്തുക` copy,
  `തുറക്കുക` open, `ഇല്ലാതാക്കുക` delete, `തിരയുക` search). This is the register-neutral command form every major
  Malayalam UI uses for action labels; it isn't rude and matches Cmdr's direct English buttons. Don't pile on politeness
  particles (`-ൂ`, `ദയവായി` "please") on ordinary buttons; reserve a softener only where the English is itself softened.

## Decision points

These are the calls specific to Malayalam, with what the majors do and the recommended default.

### Localization depth and majors (availability is itself a finding)

Malayalam is a genuinely localized UI language, but support is uneven, so there's no single "house style" to defer to:

- **Google**: full Malayalam UI (Search, Android, Gmail). Polite, mostly native-coinage register. Strongest reference.
- **Microsoft**: ships a Malayalam UI plus a public terminology set and style guide (both in the reference pile under
  `_ignored/i18n/ml-IN/`). Good for terminology arbitration.
- **Netflix, Spotify**: Malayalam UI available.
- **GNOME / Nautilus**: community-translated (Swatantra Malayalam Computing). The closest file-manager precedent, but
  dated (2017) and in legacy orthography (see chillu below) - mine it for term ideas, don't copy its encoding.
- **Apple**: does NOT ship a Malayalam display/UI language. It offers Malayalam keyboard input only (incl. macOS 26
  Tahoe transliteration input). So for a macOS-native app there is NO Apple Malayalam glossary to match; we set our own
  term for macOS-specific concepts (Quick Look, Finder, Trash), leaning on Google/Microsoft usage instead.

Confidence: high on availability. Implication: pick a consistent register ourselves (this guide) rather than expecting
to mirror "the macOS Malayalam translation," which doesn't exist.

### Script: atomic chillu, not legacy ZWJ chillu (a real encoding pitfall)

Chillu letters (pure consonants: `ൻ ർ ൽ ൾ ൺ ൿ`) have two encodings. The old way is a sequence base + virama + ZWJ (`ന` +
`്` + U+200D); the modern way (Unicode 5.1, 2008) is the single atomic character (`ൻ` = U+0D7B). They can look identical
but are different byte sequences, so they break search, sorting, dedup, and the i18n checks' string compares.

- **Always use atomic chillu (U+0D7A–U+0D7F).** Never emit base + virama + ZWJ.
- Watch the source: the Nautilus reference is almost entirely legacy ZWJ chillu (~1,300 ZWJ sequences). Microsoft's
  terminology already leans atomic. If you lift a term from Nautilus, re-type or normalize it to atomic form.
- Practical check: a Malayalam value should contain ZERO U+200D (ZWJ) for chillus. A stray ZWJ is the tell of legacy
  encoding. (NFC normalization does NOT convert legacy → atomic, so this must be done by choice.)

Use the modern reformed (post-1971) orthography throughout (split vowel signs, the script every contemporary Kerala
reader expects). Confidence: high.

### Anglicism: transliterate universal tech terms, native-coin the rest

Malayalam tech UIs borrow heavily from English, transliterated into script, but NOT uniformly: settled English nouns
stay English-in-script, while common actions and well-rooted concepts use native words. Both Microsoft and Nautilus
follow this split.

- **Transliterate (keep the English word, write it in Malayalam script)** for terms where the English is what users
  actually say: `ഫയൽ` (file), `ഫോൾഡർ` (folder), `ടാബ്` (tab). Coining an obscure native word here (e.g. `രേഖ` for file)
  reads academic and hurts comprehension. Nautilus and Microsoft both transliterate file/folder.
- **Use the native Malayalam word** for actions and concepts that have a clear, common term: `തിരയുക` (search - both
  Google and Microsoft), `പകർത്തുക` (copy), `ഒട്ടിക്കുക` (paste), `ഇല്ലാതാക്കുക` (delete), `പേരുമാറ്റുക` (rename),
  `തുറക്കുക` (open). Trash/recycle is native (`ചവറ്റുകുട്ട` / Microsoft's `പുനരുപയോഗപെട്ടി`); pick one and keep it
  consistent.
- Litmus when unsure: would a Kerala user under 40 say the English word in a Malayalam sentence? If yes, transliterate;
  if they'd naturally use the Malayalam word, use it. Record each ruling in the glossary so it isn't relitigated.

Confidence: high on the split principle; medium on individual borderline terms (e.g. "volume", "drive") - flag those to
David rather than guessing.

### Numerals: Western Arabic digits with Indian grouping

Native Malayalam numerals (`൦–൯`) are archaic - effectively unused in modern Kerala, which writes numbers in Western
Arabic digits (0–9). CLDR agrees: `Intl.NumberFormat('ml')` resolves to the `latn` numbering system.

- **Use Western Arabic digits (0–9) for all UI numbers.** Never native Malayalam numerals.
- **Grouping is Indian (lakh) style, not thousands:** CLDR formats 1234567 as `12,34,567`, not `1,234,567`. Where Cmdr
  formats a count itself rather than via `Intl`, mind this. Decimal separator is `.`. Confidence: high.

### Inclusive / gender

Malayalam verbs and the imperative form Cmdr uses are gender-neutral by default, and pronoun-free phrasing (recommended
above) sidesteps gender entirely, so the English style guide's gender-neutral requirement is naturally satisfiable. No
grammatical-gender agreement is triggered by the UI's command-form labels. Watch only for borrowed nouns or adjectives
that could carry an implied gender; prefer neutral phrasing. Confidence: high.

## Plurals

CLDR plural categories for `ml`: `one` and `other` (`new Intl.PluralRules('ml').resolvedOptions().pluralCategories`).
Every plural message must cover both. Malayalam pluralizes nouns with the `-കൾ` suffix (`ഫയൽ` → `ഫയലുകൾ`), and a counted
noun after a number commonly stays grammatically singular in form, so write each branch for natural Malayalam rather
than mirroring English plural rules. No gender or case agreement interacts with the count for UI strings.

## Brand and do-not-translate

Keep verbatim (in Latin script, not transliterated): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look,
plus the `{system_settings}`-style tokens. The curated list is enforced by `desktop-i18n-dont-translate`
(`apps/desktop/scripts/i18n-catalog-lib.ts`). Don't transliterate brand names into Malayalam script.

## Terminology and glossary

Build this up as terms come up. Starting set (encode all chillus atomically):

| English term                                   | Malayalam    | Notes                                                  |
| ---------------------------------------------- | ------------ | ------------------------------------------------------ |
| File                                           | ഫയൽ          | Transliterated; what users say.                        |
| Folder                                         | ഫോൾഡർ        | Transliterated.                                        |
| Open                                           | തുറക്കുക     | Native.                                                |
| Copy                                           | പകർത്തുക     | Native.                                                |
| Paste                                          | ഒട്ടിക്കുക   | Native.                                                |
| Delete                                         | ഇല്ലാതാക്കുക | Native.                                                |
| Rename                                         | പേരുമാറ്റുക  | Native.                                                |
| Search                                         | തിരയുക       | Native; matches Google + Microsoft.                    |
| Name                                           | പേര്         |                                                        |
| Size                                           | വലിപ്പം      |                                                        |
| Type                                           | തരം          |                                                        |
| Trash                                          | ചവറ്റുകുട്ട  | Pick one trash term and stay consistent.               |
| Tab                                            | ടാബ്         | Transliterated.                                        |
| Pane, Volume, Drive, Viewer, Listing, Transfer | (to decide)  | Flag to David: borderline transliterate-vs-coin calls. |

## Notes and decisions

- **Encoding check to add when translating (TODO):** before the `ml` catalog ships, wire an atomic-chillu validation
  guard. Every `ml` value must use the atomic chillu code points (U+0D7A–U+0D7F), never legacy base + virama + ZWJ. The
  two look identical but differ in bytes and break search, sort, and dedup; NFC does NOT convert legacy to atomic, so it
  must be enforced by check. Practical rule: a `ml` value should contain ZERO U+200D (ZWJ) used for chillus. GNOME
  reference data is almost all legacy ZWJ chillu, so re-normalize anything lifted from it. Build this check when `ml` is
  actually translated.
- Punctuation: use the Latin full stop `.`, comma `,`, and question mark `?`. Malayalam does not use distinct
  sentence-terminal punctuation; standard Latin marks are conventional in modern Malayalam UIs.
- Quotation marks: follow the English source's marks; no Malayalam-specific quote convention to enforce.
- Spacing: Malayalam doesn't capitalize, so "sentence case" rules from the English guide don't apply to letterforms;
  keep them as guidance for the English source only.
- When in doubt on register or a borderline anglicism, flag the string for David rather than guessing (principle 6:
  human-reviewed).

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ml/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
