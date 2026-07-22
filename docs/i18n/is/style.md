# Icelandic (is) translation style guide

Working notes for translating Cmdr into Icelandic (íslenska). Read `../README.md` for how this fits the translation
process, and the app-wide `docs/style-guide.md` for the English voice.

## Voice and tone

Friendly, concise, active, calm. Icelandic UI copy is direct. Error messages stay calm and actionable and avoid a bare
"Villa"/"Mistókst" label: state the problem and a next step. Icelandic has a strong language-purism tradition (native
coinages over loanwords), so prefer the established native term over an English loan where one exists.

## Formality

**Icelandic has no T-V formality distinction in practice.** "þú" (you, singular) is universal; the old plural-polite
"þér" is archaic and inappropriate for a friendly app. There is no formal/informal choice to make.

- Address: use "þú" where direct address is needed, but UI mostly uses the imperative/infinitive and avoids the pronoun.
- Buttons and menu items: imperative ("Afrita", "Færa", "Eyða", "Endurnefna", "Opna", "Leita", "Hætta við"). This
  matches Xfce Thunar Icelandic and MS terminology.

## Decision points

### Strong grammatical gender + four-case declension (the big one)

- Icelandic nouns have three genders (m/f/n) and decline through four cases (nominative, accusative, dative, genitive),
  and adjectives + articles agree. A counted noun changes case with surrounding prepositions ("í 3 möppum" = dative
  plural). A `{name}` placeholder dropped into a case slot can't be inflected by the catalog.
- Majors: no Apple Icelandic (Apple ships no Icelandic localization, see below); MS terminology and Thunar handle it by
  declining in-string and keeping placeholders in the nominative where possible.
- Recommendation: restructure sentences so placeholders stay nominative or carry their own preposition; get the case
  right inside each plural branch. This is the dominant correctness risk. Confidence: high (grammatical fact).

### No first-party Apple localization (authority shifts to Microsoft + Thunar)

- macOS does NOT ship in Icelandic; there is no `is/macOS/` reference pile (only MS terminology + Thunar + GNOME).
- So the usual Tier-1 (macOS) authority is absent. Use MS terminology (Tier 2) and Xfce Thunar / GNOME Nautilus (Tier 3)
  as the top sources, cross-checked against each other.
- Recommendation: lean on MS terminology for system terms and Thunar (a file manager) for file-operation verbs.
  Confidence: high (this is just which sources exist).

### "trash" and "volume" term traps

- The MS terminology TBX gives trash → "ruslakrafa" (likely a typo/odd entry) and volume → "hljóðstyrkur" (the
  AUDIO-loudness sense, wrong for a disk volume). Don't take these at face value.
- Thunar uses "ruslatunnu"/"rusl" for trash, which is the natural file-manager term.
- Recommendation: trash → "rusl" / "ruslafata" (confirm); disk volume → "diskur" or keep a clearer term, NEVER
  "hljóðstyrkur". Confidence: tentative (sources conflict); flag for David.

### No regional variant

- Icelandic is `is` only (one country, one standard). No region split. Confidence: high.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. With no macOS source, tier order here is MS terminology (top) → Xfce
Thunar → GNOME Nautilus. From mined `is/microsoft-terminology/ICELANDIC.tbx` and `is/xfce-thunar/thunar.po`.

- file → skrá (f) · MS, Thunar · high
- folder → mappa (f) · MS · high
- directory → skráasafn / mappa · MS ("skráasafn", technical) · high
- drive → drif · MS · high
- delete → eyða ("Eyða") · MS, Thunar · high
- copy → afrita ("Afrita") · MS, Thunar · high
- move → færa ("Færa") · Thunar; MS terminology gives "hreyfa" (the physical-motion sense, less apt). Prefer "færa". ·
  high
- rename → endurnefna ("Endurnefna") · Thunar · high
- open → opna ("Opna") · MS, Thunar · high
- search → leita (verb) / leit (noun) · MS · high
- cancel → hætta við ("Hætta við") · MS · high
- trash → rusl / ruslafata · Thunar ("ruslatunnu"); MS "ruslakrafa" looks wrong, do not use · tentative
- settings → stillingar · MS · high
- disconnect → aftengja · MS · high
- server → þjónn · MS · high
- volume (disk) → diskur / disksneið · NOT "hljóðstyrkur" (audio sense in MS TBX) · tentative
- directory → skráasafn · MS · high
- replace / overwrite → skrifa yfir / skipta út · MS · tentative
- eject → spýta út / fjarlægja · confirm · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `other` (verified `new Intl.PluralRules('is')`, 2026-06-20). Note Icelandic's `one` rule is
unusual: it covers any number ending in 1 EXCEPT those ending in 11 (so 21, 31, 101 are `one`; 11, 111 are `other`).
CLDR/ICU encodes this; write both branches and trust ICU's selection. Each branch must decline the counted noun in the
correct case and gender.

## Notes and decisions

- **Quotation marks: `„…“`** (low-high, like German) is the Icelandic standard. Avoid straight `"…"`.
- **Special characters:** Icelandic uses þ, ð, æ, ö and accented vowels (á é í ó ú ý). Ensure these survive the catalog
  round-trip; never strip diacritics.
- **Length:** Icelandic runs roughly comparable to English, sometimes longer for compounds. Overflow-check against the
  pseudolocale (`en-XA`).
- **Numbers/dates** come from the formatter layer (comma decimal, period thousands). Never hardcode.
- **Compounding:** Icelandic forms long compound nouns ("ruslatunnu" = trash+can). Correct, but watch length.

## Decisions to confirm with David

- **trash term** (rusl vs ruslafata vs ruslatunnu) and **volume (disk)** term (the MS TBX "hljóðstyrkur" is the audio
  sense, definitely wrong): both tentative, sources thin/conflicting. Confirm with a native speaker.
- **Should Cmdr even ship Icelandic?** Apple ships no Icelandic localization, so there's no native macOS reference and a
  smaller user base. Lower priority than the major Latin/CJK locales; flag the prioritization to David.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/is/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
