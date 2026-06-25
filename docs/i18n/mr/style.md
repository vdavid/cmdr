# Marathi (mr) translation style guide

Working notes for translating Cmdr into Marathi. Read [`README.md`](../README.md) for how this fits the translation
process. `mr` is the base tag (the universal Marathi set); `mr-IN` would only exist for region-specific overrides, and
none are needed today (Maharashtra usage is the de-facto standard and what `mr` targets). Read the Decision points
first: the script pitfalls (Marathi-only letters) and the formality default dominate the rest.

## Voice and tone

Friendly, concise, calm, active. Same register Cmdr uses in English: a helpful peer, never bureaucratic or alarmist.
Error and warning copy stays calm and actionable; never use a Marathi equivalent of "error" or "failed" as a scare word
(prefer plain phrasings like `अडचण आली` "ran into a problem" over `त्रुटी` / `अयशस्वी`). Prefer plain everyday Marathi
(the register of a well-made consumer app), not heavily Sanskritized prose. Marathi is verb-final (SOV); let the action
verb close the sentence rather than forcing English word order.

## Formality

Marathi marks respect through the second-person PRONOUN and the verb ending: `तू` (informal) vs `तुम्ही`
(polite/plural).

- **Use the `तुम्ही` register.** Crucially, unlike Hindi, Marathi `तू` is the everyday default among family and peers
  and is NOT inherently disrespectful, but addressing a stranger (which every app user is) with `तू` reads too familiar
  and can offend. `तुम्ही` is the safe, neutral choice for someone whose age and status you don't know, and it's what
  Google and Microsoft use in their Marathi UIs.
- **Prefer pronoun-free phrasing.** Most UI copy needs no "you" at all; Marathi drops the subject readily. "Are you sure
  you want to delete?" reads best without a pronoun. This also sidesteps gender agreement (see Decision points).
- **Imperatives for actions (buttons, menu items): use the `तुम्ही`-imperative `-आ` ending** (e.g. `कॉपी करा` copy,
  `उघडा` open, `चिकटवा` paste, `काढून टाका` delete, `बदला`/`नाव बदला` rename, `शोधा` search). This is the
  register-neutral command form every major Marathi UI uses for action labels (Microsoft `रद्द करा` cancel, `बंद करा`
  close, `क्रमवारी लावा` sort; Nautilus `उघडा`, `चिकटवा`, `कापा`). The plain `-ए`/`-` stem (`कर` "do") is the `तू` form
  and reads rude on a button; never use it. Don't pile on `कृपया` ("please") on ordinary buttons; reserve a softener
  only where the English is itself softened.

## Decision points

These are the calls specific to Marathi, with what the majors do and the recommended default.

### Localization depth and majors (availability is itself a finding)

Marathi is a genuinely localized UI language with strong precedent, but it's uneven, so there's no single "house style"
to wholesale-copy:

- **Google**: full Marathi UI (Search, Android, Gmail, Maps). Polite `तुम्ही` register, mix of native words and
  transliterated tech nouns. Strongest reference.
- **Microsoft**: ships a Marathi UI plus a public terminology set (`MARATHI.tbx` in the reference pile) and a style
  guide. Best source for terminology arbitration. Uses the `-आ` imperative and `तुम्ही` register.
- **Spotify**: added Marathi to its mobile app (one of 36 Indic-heavy additions). UI available.
- **GNOME / Nautilus**: community-translated. The closest file-manager precedent for term ideas (`कचरापेटी` trash,
  `फोल्डर` folder, `फाइल` file, `शोधा` search), but dated and uneven; mine it, don't copy verbatim.
- **Netflix**: Marathi UI availability is unconfirmed; treat as not a reference.
- **Apple**: does NOT ship a Marathi display/UI language. iOS offers Marathi keyboard input and dictation; macOS offers
  Marathi keyboard input but not even dictation. So for a macOS-native app there is NO Apple Marathi glossary to match;
  we set our own term for macOS-specific concepts (Quick Look, Finder, Trash), leaning on Google/Microsoft usage.

Confidence: high on availability. Implication: pick a consistent register ourselves (this guide) rather than expecting
to mirror "the macOS Marathi translation," which doesn't exist.

### Script: use the Marathi-specific letters, not the Hindi subset (a real correctness pitfall)

Marathi uses Devanagari, but with letters and conventions Hindi lacks. A Hindi-trained translator or model (or a font
with thin Marathi coverage) silently substitutes the Hindi forms, which reads wrong to a Marathi user and breaks string
compares.

- **`ळ` (U+0933, retroflex LLA) is Marathi-only** and absent from standard Hindi. Use it wherever the word has the
  retroflex `l` (place and word forms, many native terms); never flatten it to `ल` (U+0932).
- **The eyelash-ra `ऱ` (U+0931, "dhrav-ra") is a distinct Marathi letter.** It's NOT ordinary `र` (U+0930) plus rakar,
  and not the `र्` repha. It renders as the small eyelash mark; encode it as the dedicated code point, not by faking it
  with a ZWJ/joiner sequence, or it breaks search, sort, and dedup. It's rarer in UI copy than `ळ`, but when a source
  term needs it, get the code point right.
- **Conjuncts**: `क्ष` (kṣa) and `ज्ञ` (jña) are common; render them as proper conjuncts via the virama, not as broken
  half-forms. NFC-normalize all values. Modern reformed orthography throughout (the script a contemporary Maharashtra
  reader expects).

Confidence: high that these are correctness issues; the practical risk is a translator defaulting to Hindi habits, so a
native Marathi reviewer should specifically check `ळ` vs `ल`.

### Anglicism: transliterate universal tech terms, native-coin the rest

Marathi tech UIs borrow heavily from English, transliterated into Devanagari, but NOT uniformly: settled English nouns
stay English-in-script, while common actions and well-rooted concepts use native words. Microsoft and Nautilus both
follow this split.

- **Transliterate (keep the English word, write it in Devanagari)** for terms users actually say: `फाइल` / `फाईल`
  (file), `फोल्डर` (folder), `टॅब` (tab), `रीसायकल बिन` (Microsoft's Recycle Bin), `ड्रायव्हर` (driver). The native
  coinages exist (`संचिका` file, `धारिका`/`निर्देशिका` folder) but read academic/officialese in a consumer app; both
  Microsoft and Nautilus transliterate file/folder. Default to the transliteration.
- **Use the native Marathi word** for actions and concepts with a clear common term: `शोधा` (search, both Google and
  Microsoft), `चिकटवा` (paste), `काढून टाका` (delete), `उघडा` (open), `बंद करा` (close), `रद्द करा` (cancel),
  `क्रमवारी लावा` (sort). Copy is borderline: Nautilus uses native `प्रत बनवा` while many UIs use transliterated
  `कॉपी करा`; pick one and stay consistent. Trash is native `कचरापेटी` (Nautilus); macOS calls it Trash, so confirm
  whether to match Apple's English or use `कचरापेटी`.
- Litmus when unsure: would a Maharashtra user under 40 say the English word in a Marathi sentence? If yes,
  transliterate; if they'd naturally use the Marathi word, use it. Record each ruling in the glossary.

Confidence: high on the split principle; medium on borderline terms (`copy`, `volume`, `drive`, `pane`) - flag those to
David rather than guessing.

### Numerals: Western Arabic digits with Indian (lakh) grouping

Devanagari numerals (`०–९`) exist and are still seen, but modern Marathi software and CLDR default to Western Arabic
digits. CLDR's default numbering system for `mr` is `latn` (`Intl.NumberFormat('mr')` resolves to Western digits);
Devanagari digits are available only via an explicit `-nu deva` override.

- **Use Western Arabic digits (0–9) for all UI numbers.** Don't emit Devanagari numerals unless David explicitly wants
  the native-numeral look (a deliberate, all-or-nothing choice).
- **Grouping is Indian (lakh) style, not thousands:** CLDR formats 1234567 as `12,34,567`, not `1,234,567`. Where Cmdr
  formats a count itself rather than via `Intl`, mind this. Decimal separator is `.`. Confidence: high.

### Inclusive / gender: the standout Marathi pitfall

Marathi has THREE grammatical genders (masculine, feminine, neuter) and finite verbs agree with the subject's gender and
number. This is the highest dynamic-string risk in Marathi:

- A sentence whose verb agrees with a runtime-inserted noun (a file name, an item the user picked) can't be written
  gender-correctly, because the gender of `{name}` isn't known at translation time.
- **Two natural escape hatches Cmdr's UI already favors:** (1) the `-आ` imperative used for buttons/actions is
  gender-neutral, and (2) future-tense verb forms do NOT mark gender in Marathi. Past-tense and completed-action
  phrasings are where gender agreement bites.
- **Recommended defaults:** prefer pronoun-free, imperative, or future-tense phrasing; avoid past-tense constructions
  that force the verb to agree with an uncontrolled `{placeholder}`. For a status like "Copied {name}", structure it so
  the verb doesn't agree with `{name}` (lead with the action noun, or use a neutral completion phrasing) rather than a
  past participle agreeing with the file. When a string genuinely can't avoid agreement, flag it for David and a native
  reviewer.

Confidence: high that this is the key pitfall; medium on individual rewrites (a native reviewer tunes them).

## Plurals

CLDR plural categories for `mr`: `one` and `other` (verify with
`new Intl.PluralRules('mr').resolvedOptions().pluralCategories`). Every plural message must cover both. Marathi
pluralizes nouns by gender-dependent suffix changes (not a single English-style `-s`), and a counted noun's form can
shift with gender, so write each branch for natural Marathi rather than mirroring English. The gender agreement note
above also applies to the counted noun and its verb.

## Brand and do-not-translate

Keep verbatim (in Latin script, not transliterated): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look,
plus the `{system_settings}`-style tokens. The curated list is enforced by `desktop-i18n-dont-translate`
(`apps/desktop/scripts/i18n-catalog-lib.ts`). Don't transliterate brand names into Devanagari.

## Terminology and glossary

Build this up as terms come up. Starting set:

| English term                                   | Marathi              | Notes                                                             |
| ---------------------------------------------- | -------------------- | ----------------------------------------------------------------- |
| File                                           | फाइल                 | Transliterated; what users say. Watch `ळ` if a spelling needs it. |
| Folder                                         | फोल्डर               | Transliterated.                                                   |
| Open                                           | उघडा                 | Native; `-आ` imperative.                                          |
| Copy                                           | कॉपी करा / प्रत बनवा | Borderline: transliterated vs Nautilus's native. Pick one.        |
| Paste                                          | चिकटवा               | Native.                                                           |
| Cut                                            | कापा                 | Native (Nautilus).                                                |
| Delete                                         | काढून टाका           | Native.                                                           |
| Rename                                         | नाव बदला             | Native.                                                           |
| Search                                         | शोधा                 | Native; matches Google + Microsoft.                               |
| Cancel                                         | रद्द करा             | Native (Microsoft).                                               |
| Close                                          | बंद करा              | Native (Microsoft).                                               |
| Sort                                           | क्रमवारी लावा        | Native (Microsoft).                                               |
| Name                                           | नाव                  |                                                                   |
| Size                                           | आकार                 |                                                                   |
| Trash                                          | कचरापेटी             | Native (Nautilus); confirm vs matching macOS "Trash".             |
| Recycle Bin                                    | रीसायकल बिन          | Microsoft transliterates.                                         |
| Tab                                            | टॅब                  | Transliterated.                                                   |
| Pane, Volume, Drive, Viewer, Listing, Transfer | (to decide)          | Flag to David: borderline transliterate-vs-coin calls.            |

## Notes and decisions

- Punctuation: use the Latin full stop `.`, comma `,`, and question mark `?`. The traditional Devanagari danda `।` is
  not used as sentence-terminal punctuation in modern Marathi UIs; standard Latin marks are conventional.
- Quotation marks: follow the English source's marks; no Marathi-specific quote convention to enforce.
- Capitalization: Devanagari has no letter case, so the English guide's "sentence case" rules don't apply to
  letterforms; keep them as guidance for the English source only.
- When in doubt on register, a borderline anglicism, the `ळ`/`ल` choice, or a gender-agreement rewrite, flag the string
  for David and a native reviewer rather than guessing (principle 6: human-reviewed).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/mr/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
