# Burmese (my) translation style guide

Working notes for translating Cmdr into this language. Read [`README.md`](../README.md) for how this fits the
translation process.

Burmese (Myanmar language, autonym မြန်မာ) is the official language of Myanmar, a Sino-Tibetan language written in the
Myanmar abugida (Unicode block U+1000–U+109F). The BCP-47 base tag is `my`. **Read the Decision points first: the
Zawgyi-vs-Unicode encoding choice dominates everything else here and is the single biggest Burmese digital-text
pitfall.**

## Voice and tone

Friendly, concise, active, calm. Match Cmdr's English register. Burmese signals politeness mainly through sentence-final
particles and verb endings rather than loud wording, so a polite-but-plain register reads naturally for UI. Keep
sentences short and clear. Error messages stay calm and actionable and avoid alarm (Cmdr's English never uses the words
"error" or "failed"); Burmese has no fixed software-UI house style to inherit, so the translator sets a plain, modern
tone. GNOME Files (Nautilus) has a recently maintained Unicode Burmese translation and is the closest living
file-manager-UI precedent for register and terminology.

## Formality

Use the polite register, but lightly. Burmese marks politeness with the sentence-final particle ပါ (pa) and the polite
verb ending တယ်/သည်; attach ပါ to imperative UI actions to make them courteous without being verbose (for example
"select" reads as ရွေးပါ, "sign in" as ဝင်ရောက်ပါ in Microsoft's terminology). Burmese culturally prefers pronoun
avoidance: don't reach for an explicit "you" pronoun where the sentence works without one, which is the norm for UI
copy. Keep button and menu labels as short polite imperatives. There's a colloquial/formal split in subject and object
markers (colloquial က / ကို vs formal သည် / အား); UI copy uses the everyday polite register, not the literary-formal
one. Confidence: medium-high on the polite-particle approach (matches Microsoft and Nautilus), lower on per-string
particle density, which a native reviewer should tune.

## Terminology and glossary

| English term | This language  | Notes                                                                                                |
| ------------ | -------------- | ---------------------------------------------------------------------------------------------------- |
| file         | ဖိုင်          | Transliteration of "file"; universal in Burmese computing (Nautilus, Microsoft).                     |
| folder       | ဖိုင်တွဲ       | Literally "file-bundle"; the established Nautilus term.                                              |
| copy         | ကူးယူ / မိတ္တူ | Native verb ကူးယူ for the action; မိတ္တူ ("duplicate/copy") also seen. Pick one and stay consistent. |
| open         | ဖွင့်          | Native verb.                                                                                         |
| rename       | အမည်ပြောင်း    | Native ("change name").                                                                              |
| trash        | အမှိုက်ပုံး    | Native ("rubbish bin").                                                                              |
| cancel       | ပယ်ဖျက်        | Native.                                                                                              |

Fill more as they come up. Expect heavy English borrowing for tech-specific terms (file, tab, app, email → အီးမေးလ်,
screenshot → ဖန်သားပြင်ရိုက်ချက်); use a clear native word where one is widely understood (open, rename, trash, copy),
otherwise transliterate. Record every call here so spelling stays consistent: the same loanword can be transliterated
several ways, so a native reviewer should confirm spelling. Microsoft's Burmese terminology
(`_ignored/i18n/my/microsoft-terminology/BURMESE.tbx`) and the Nautilus catalog are the reference pile, for DECIDING,
never copy verbatim.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, and the `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

`new Intl.PluralRules('my').resolvedOptions().pluralCategories` reports a single category: `other`. Burmese has no
grammatical number agreement (a noun's form doesn't change with count, and there's no gender), so every plural message
needs only the `other` branch. Nautilus's Burmese catalog ships `nplurals=1`, confirming this. Counts are typically
expressed with a classifier construction rather than a plural marker, but for UI counts the `{count}` placeholder plus a
noun reads fine. Verify the category the runtime returns for `my` before writing branches.

## Decision points

These are the calls that actually matter for Burmese; settle them before a translation pass.

- **Encoding check to add when translating (TODO):** before the `my` catalog ships, wire a Zawgyi-vs-Unicode validation
  guard. Every `my` value must be strict, NFC-normalized Unicode (Myanmar block U+1000–U+109F), never Zawgyi. Zawgyi
  shares the same code points, so a string can LOOK Burmese yet be wrong bytes that render as garbage. Detect with
  Google's `myanmar-tools` (npm `myanmar-tools`) `ZawgyiDetector` over the whole catalog, fail on any Zawgyi hit, and
  NFC-normalize. This is the one Burmese-specific check worth adding; build it when `my` is actually translated.

- **Encoding: strict Unicode (NFC), never Zawgyi (the headline finding).** Two encodings compete for Burmese text on the
  same Myanmar code points. Zawgyi (the legacy "Zawgyi-One" font encoding) is NOT Unicode-compliant: it stores
  characters in a different, overlapping arrangement of the U+1000–U+109F block, so Zawgyi-encoded bytes render as
  garbage in a Unicode-expecting renderer and vice versa. The two are mutually unintelligible at the byte level even
  though both look like "Burmese". Myanmar's official migration to proper Unicode began with government adoption in 2019
  ("U-Day", 1 October 2019), pushed hard by Google and Facebook (Facebook shipped auto-conversion that year). Unicode is
  now the standard direction, but Zawgyi remains common on older devices and content in Myanmar, so the ambiguity is
  real and ongoing.
  - **Recommendation: the catalog MUST be strict, NFC-normalized Unicode (Myanmar block), never Zawgyi.** This is
    non-negotiable, not a preference: Cmdr renders text with the OS/webfont Unicode shaper, so Zawgyi bytes would render
    as garbage, and Zawgyi is a dead-end encoding outside its bespoke font. Confidence: very high.
  - **Validate source strings as Unicode.** Because Zawgyi and Unicode share code points, a string can LOOK Burmese yet
    be Zawgyi-encoded, and an agent or copy-pasted reference can silently introduce Zawgyi. Validate every translated
    value is genuine Unicode before it lands. Google's `myanmar-tools` (npm `myanmar-tools`, also C++/Java/PHP/Ruby/
    Dart/C#) provides an ML-based `ZawgyiDetector` and a Zawgyi-to-Unicode converter; run detection over the `my`
    catalog as a guard, and normalize to NFC. This is the one Burmese-specific check worth wiring. David-only call on
    whether to add it as a repo check. Confidence: very high on the need, medium on the exact tooling integration.

- **Script rendering depends on correct shaping.** Burmese is a complex script: base consonants stack with subjoined
  consonants (virama U+1039, never shown as its own glyph), medials reorder (medial ya/ra/wa/ha), and vowel signs and
  tone marks attach around the base. Correct display needs a proper OpenType shaper plus a Unicode-compliant Myanmar
  font; the glyph order on screen is not the code-point order in the string. Practical effect for Cmdr: rely on the OS
  shaper (macOS ships a capable Myanmar shaper and the Myanmar Sangam MN / Noto Sans Myanmar fonts), and if bundling a
  webfont, use a Unicode-compliant one (Noto Sans Myanmar, free, SIL OFL), NEVER a Zawgyi font. Don't hand-manipulate or
  reorder the characters in a translated string to "fix" rendering; that means the font or shaper is wrong, not the
  text. Confidence: high.

- **Numerals: default to Western Arabic in the UI.** Burmese has its own digits (၀၁၂၃၄၅၆၇၈၉, U+1040–U+1049) and CLDR's
  `my` default numbering system is `mymr` (Burmese digits). But Western Arabic numerals are widely used and accepted in
  digital, technical, and international contexts in Myanmar, and Nautilus's Burmese catalog leaves numbers as Western
  digits (it passes `%d`-style counts through unchanged and uses no Burmese digits in static copy). For a technical
  file-manager UI (sizes, counts, dates, paths), Western Arabic numerals are the safer, more legible default and avoid a
  mismatch between Burmese digits in static text and Western digits in OS-provided values (file sizes, paths). Most
  numbers in Cmdr arrive via `{count}`-style placeholders anyway, so they're whatever the runtime formats, not the
  translator's choice. Recommendation: Western Arabic numerals; don't convert placeholder-fed numbers to Burmese digits.
  Cmdr's house ISO-date default (YYYY-MM-DD) holds. Confidence: medium-high; a David-or-reviewer call if a
  Burmese-numeral feel is ever wanted.

- **Word spacing and line breaking: no inter-word spaces.** Burmese writes words run together with no space between
  them; a space in Burmese is a phrase or clause separator, not a word boundary. This is a real line-breaking pitfall:
  naive break-on-space logic produces wrong breaks (or no breaks) in long Burmese strings, and overflow in narrow UI
  (pane columns, dialogs) can clip. Don't sprinkle ordinary spaces between words to force breaks; that changes meaning.
  Rely on the renderer's Myanmar line-break logic (the macOS text engine does syllable-aware breaking). Zero-width space
  (U+200B) can mark break opportunities, but the modern Nautilus catalog inserts none and leans on the shaper, so don't
  hand-insert ZWSP in catalog values by default. Watch overflow during the pseudolocale/layout pass especially for
  Burmese, since strings won't break where Latin ones would. Confidence: high on the convention, medium on whether any
  manual break hints are ever needed (treat as a per-overflow fix, not a default).

- **Anglicism / tech-term borrowing.** Burmese borrows computing vocabulary from English heavily; Microsoft's Burmese
  terminology transliterates email, screenshot, domain, virus, and similar directly. Expect "file", "tab", "app", and
  many tech terms to be transliterations, while a clear native word exists for common verbs (open ဖွင့်, rename
  အမည်ပြောင်း, copy ကူးယူ, trash အမှိုက်ပုံး). Default to the established transliteration for tech-specific nouns and a
  native verb for actions, record each call in the glossary, and have a native reviewer confirm spelling. Confidence:
  high that borrowing is the norm, lower on individual spellings.

- **Localization depth / availability (priority signal).** Major-vendor Burmese UI is moderate-to-thin. Apple does NOT
  ship a Burmese macOS/iOS interface (input and fonts only), so there's no Apple-platform precedent to mirror, which
  matters for an OS-native macOS app. Microsoft ships Burmese terminology and a privacy-content style guide but limited
  full-product UI; Google localizes Burmese (Search, Translate, Android, and the myanmar-tools investment); Facebook was
  historically the biggest driver of the Unicode migration. Spotify and Netflix do not ship a Burmese interface (Netflix
  offers some Burmese content/subtitles, not a localized UI; Spotify isn't officially launched in Myanmar). So there's
  enough living Unicode UI vocabulary to translate confidently (Google, Microsoft terminology, Nautilus), but no
  macOS-native reference and a moderate addressable base. Recommendation: viable but not high priority; ship with a
  native reviewer and the Unicode validation guard in place. David-only call. Confidence: high.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/my/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
