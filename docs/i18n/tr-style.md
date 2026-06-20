# Turkish (tr) translation style guide

Working notes for translating Cmdr into Turkish. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into
Turkish. Turkish is a single-script (Latin), no-region-split, well-localized (Tier 1) language, so the sources are rich
and mostly agree; the open calls are tonal, not lexical.

## Voice and tone

Friendly, concise, active, calm, and never alarmist, the same register as the English. Turkish UI copy can drift into
old-style formal officialese; resist it. The Microsoft Turkish voice guidance is explicit: avoid archaic, overly serious
words, prefer everyday vocabulary, keep inflections simple, and write as if you were writing the content natively rather
than translating word for word (verified in `tr/microsoft-style-guides/StyleGuide.pdf`, 2026-06-20).

- Turkish is agglutinative: one word often carries a whole English phrase, so translations frequently come out shorter
  per word but can still run 20-30% longer overall. Overflow-check the layout against the pseudolocale (`en-XA`).
- Error messages stay calm and actionable and never lead with "hata" (error) or "başarısız" (failed) as a bare label.
  State the problem and a next step. Note: macOS itself does use "bir hata oluştu" freely; Cmdr's voice rule is stricter,
  so don't copy that pattern.
- Prefer a verb over a verbal noun where the English does ("Ara", not a noun phrase for "Make a search").

## Formality: `sen` (informal), settled

**Verdict: informal second person (`sen`, the singular verb endings `-ın` / `-in` / `-musun` / `-misin`).** Consumer
brands (Trendyol, Spotify, Duolingo, and peers) address Turkish users informally with `sen`, which fits Cmdr's
friendly personal voice. This bucks the OS-vendor norm: macOS and Microsoft both use formal `siz`, but Cmdr
deliberately picks the warmer consumer-brand register. Formality decision recorded in
[`formal-informal-decisions.md`](formal-informal-decisions.md).

- The OS sources lean formal: macOS Turkish formal `siz` markers outnumber informal `sen` 409 to 32 in the mined
  Finder strings, and AppKit is entirely formal; Microsoft Turkish is formal-plural too. Cmdr departs from this on
  purpose to match how Turkish consumer apps speak.
- So phrase prompts in the informal singular: "… değiştirmek ister misin?", "… diski seç", rather than the formal
  "ister misiniz?" / "seçin".

### Imperatives in buttons and menu items

Single-action button and menu labels use the bare second-person-singular imperative stem (no honorific suffix), which is
the standard short UI form: "Sil" (Delete), "Kopyala" (Copy), "Taşı" (Move), "Aç" (Open), "Çıkar" (Eject), "Ara"
(Search), "Sırala" (Sort), "Vazgeç" (Cancel). This is what macOS Finder uses for command labels, and it aligns directly
with the informal `sen` register. So: short labels = bare imperative; full sentences to the user = informal `sen`. Both
are the same singular grammatical person, fully consistent.

## Decision points

### Dotted/dotless i and locale-aware case mapping (high)

Turkish has four i's: dotted `i`/`İ` and dotless `ı`/`I`. They are distinct letters, and case mapping is locale-specific:
under a Turkish locale `"i".toUpperCase()` is `"İ"` (not `"I"`) and `"I".toLowerCase()` is `"ı"` (not `"i"`). This is the
infamous Turkish locale bug, and it cuts both ways for Cmdr:

- **Any case-changing of a Turkish UI string must be locale-aware.** If code ever uppercases or lowercases a translated
  label for display (a CSS `text-transform` is fine, it's presentational; a JS `.toUpperCase()` on the string is not),
  it must use `toLocaleUpperCase('tr')` / `toLocaleLowerCase('tr')`, or it will mangle Turkish words.
- **Conversely, locale-INSENSITIVE comparisons must force a neutral locale.** Anything that case-folds for matching,
  routing, or token comparison (file extensions, brand tokens, the don't-translate check, ICU keyword matching) must use
  `toLowerCase('en')` / a non-Turkish locale, or `"FILE".toLowerCase()` becomes `"fıle"` on a Turkish machine and
  silently breaks the match. This is a real, shipped-bug class (Java, PHP, .NET, Kotlin, Dart have all hit it).
- Apple, Microsoft, and Google all handle this by separating display case-mapping (locale-aware) from
  identifier/comparison case-folding (invariant). Recommendation: never `.toUpperCase()`/`.toLowerCase()` a Turkish
  string with the default locale; pick locale-aware for display and invariant for comparison, explicitly. Confidence:
  high. Flag for David: this is a code-correctness item, not a translation choice; worth a one-line guardrail in the
  intl runtime docs so it isn't rediscovered as a bug.

### Suffixes on placeholders: avoid attaching them (high)

Turkish marks grammatical case with suffixes that obey vowel harmony, and proper nouns take an apostrophe before a
suffix ("Ali'yi", "Mac'i"). A filename or path in a `{name}` placeholder is uncontrolled text, so the catalog can't pick
the right harmonized suffix or decide the apostrophe. Don't write "{name}'i sil" (the `-i` is wrong after a
back-vowel name, and the apostrophe rule varies). Instead, follow exactly what macOS Finder does: put a generic noun
after the quoted placeholder and attach the suffix to THAT noun:

- macOS pattern: `"^0" öğesini açamazsınız` ("you can't open the item ^0"), `"^0" uygulamasını …` ("the application
  ^0 …"), `"^0" içindeki öğeler` (verified in `tr/macOS/Finder/`, 2026-06-20). The suffix lands on `öğe` / `uygulama`
  (item / application), never on the placeholder, so vowel harmony and the apostrophe are both sidestepped.
- Recommendation: structure every placeholder-bearing Turkish string so the inflected suffix sits on a fixed Cmdr-chosen
  noun ("öğe", "dosya", "klasör"), with the `{name}` in quotes and uninflected. This is the single highest
  blind-translation risk for this language. Confidence: high.

### Number and date formatting (high)

Turkish uses a decimal comma and a dot (or space) for thousands: `1.234.567,89`. Dates are `dd.MM.yyyy` (20.06.2026),
24-hour time. This is consistent across Apple, Microsoft, and Google for tr-TR. Recommendation: never hardcode
separators in catalog strings; all numbers, sizes, counts, and dates come from the `Intl`/formatter layer with the
locale tag. Confidence: high.

### Cancel: `Vazgeç` vs `İptal` (tentative, macOS-vs-Windows split)

macOS Turkish uses **Vazgeç** for Cancel (47 occurrences in Finder, zero "İptal"). Windows/Microsoft and GNOME Nautilus
use **İptal**. Since Cmdr is a macOS app, recommend **Vazgeç** to match what a macOS user sees in native dialogs; note
İptal is what a Windows-trained user expects. Confidence: high for the macOS-native pick, but flagged for David since
both are correct and it's a recognizability call.

### No grammatical gender (high, simplifying)

Turkish has no grammatical gender, no gendered pronouns, and no article agreement. The third-person pronoun `o` covers
he/she/it/they. This removes a whole class of agreement problems other languages have. The only inclusive-language note
is lexical: prefer role/neutral nouns over borrowed gendered ones where they arise (Microsoft flags "kişi"/"birey" over
gendered borrowings), but for a file manager this rarely comes up. Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order: macOS (Tier 1) > Microsoft (Tier 2) >
GNOME/Xfce (Tier 3). Not exhaustive this round; extend as terms come up.

- file → dosya · macOS, MS terminology, Nautilus · high
- folder → klasör · macOS, MS terminology · high
- delete → Sil · macOS Finder · high
- copy → Kopyala · macOS Finder · high
- move → Taşı · macOS Finder · high
- rename → Yeniden Adlandır · macOS Finder · high
- open → Aç · macOS · high
- eject → Çıkar · macOS Finder · high
- cancel → Vazgeç · macOS (vs Microsoft/Nautilus "İptal", see decision point above) · high
- trash → Çöp Sepeti · macOS Finder (consistent) · high
- search → Ara (verb) · macOS Finder · high
- sort → Sırala · macOS · high
- settings → Ayarlar · macOS · high
- sidebar → Kenar Çubuğu · macOS Finder · high
- connect (to server) → Bağlan · macOS Finder · high
- item → öğe · macOS (pervasive: "öğeleri Çöp Sepeti'ne taşır", "^0 öğe") · high
- pane / tab / volume / listing / transfer / viewer / bookmark → not yet triangulated; add with sources when first used.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`). macOS UI names
Cmdr opens into (System Settings panes, "Çöp Sepeti") should match a Turkish macOS.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('tr')`, 2026-06-20). Write both branches.

- **Key grammar note: the counted noun stays SINGULAR after a number in Turkish.** "3 dosya", not "3 dosyalar". The
  plural suffix `-lar`/`-ler` is NOT added when a number precedes the noun. Real catalogs reflect this: GNOME Nautilus
  Turkish ships `nplurals=1` (a single form), with `%u dosya` used for any count (verified in `tr/gnome-nautilus`,
  2026-06-20). So in practice the `one` and `other` ICU branches usually carry the SAME noun form; the only reason to
  differ is when the surrounding sentence (not the noun) changes with count.
- Turkish has no gender or case agreement that varies by the plural category, so the branches are simple. The risk is the
  opposite of most languages: don't "correct" the `other` branch into a `-lar`/`-ler` plural to match English. Keep the
  noun singular in both.

## Notes and decisions

- **Quotation marks: macOS Turkish uses curly `"…"`** (U+201C / U+201D), e.g. `"^0" öğesini açamazsınız`. Prefer these
  over straight `"…"` for quoted names in copy.
- **Suffix-on-noun, not-on-placeholder** is the load-bearing structural rule (see decision point); repeated here because
  it shapes how nearly every `{name}`-bearing string is phrased.
- **Numbers and dates come from the formatter layer** (decimal comma, dot thousands, dd.MM.yyyy). Never hardcode.
- **ICU apostrophe escaping bites harder here**: Turkish suffixed proper nouns and contractions use apostrophes
  ("Mac'i", "Ali'nin"), and ICU swallows a lone `'`. Double every apostrophe in ICU values (`'` → `''`); in raw
  `errors.*` strings use a single normal apostrophe. (Cross-language ICU rule; flagged because Turkish uses apostrophes
  more than most.)
- Record case-by-case rulings here as they're made.

## Decisions to confirm with David

- **Cancel → Vazgeç vs İptal**: macOS-native "Vazgeç" recommended, but "İptal" is widely recognized (Windows, web). A
  recognizability call only David can settle.
- **Locale-aware casing guardrail**: the dotted/dotless i bug is a code-correctness item (not a translation choice).
  Recommend a one-line guardrail in the intl runtime docs that any case transform of a UI string must choose locale-aware
  (display) or invariant (comparison) explicitly. Confirm whether to add it now.
