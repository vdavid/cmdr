# Manipuri / Meitei (mni) translation style guide

Working notes for translating Cmdr into this language. Read `../README.md` for how this fits the translation process.

Manipuri (Meiteilon, autonym Meitei) is a Tibeto-Burman language of Manipur, north-east India, plus Assam, Tripura, and
Bangladesh. The BCP-47 base tag is `mni`; the script is carried by a `-Mtei` (Meitei Mayek) or `-Beng` (Bengali) suffix
when it must be explicit. Read the Decision points first: the script choice and the low-availability signal dominate
everything else here.

## Voice and tone

Friendly, concise, active, calm. Match Cmdr's English register. The only major-vendor precedent (Microsoft's 2025
Manipuri guide) sets the register as "formal, but friendly": warm and relaxed, short clear sentences, no literal
translation, no slang. That lines up well with Cmdr's voice. Error messages stay calm and actionable, avoiding alarm.
There's no broad software-UI register for Manipuri, so the translator largely sets the house style; keep it plain and
modern.

## Formality

Meitei has an elaborate politeness system, but only the second person carries a formal/polite variety (first and third
person don't distinguish). Use the polite second-person pronoun অদোম / ꯑꯗꯣꯝ (adom) for the user. This matches
Microsoft's choice and the cultural norm of addressing someone whose age and status you don't know with the polite form.
Politeness also shows up in polite verb suffixes; apply them as the natural register, not heavily, so UI actions stay
short. Imperatives for buttons and menu items should read as plain, direct actions. Confidence: medium-high on the
pronoun (clear precedent), lower on suffix density (a native reviewer should tune it).

## Terminology and glossary

| English term | This language | Notes |
| ------------ | ------------- | ----- |

(Fill as terms come up. Expect heavy borrowing: Manipuri transliterates most computing and tech terms from English
rather than coining native words. Microsoft's examples transliterate platform, website, version, restaurant, and
technologies directly. Decide per term whether a clear native word exists; default to the borrowed transliteration for
tech-specific terms and record every call here so spelling stays consistent. The normative dictionaries Microsoft cites
are DOLEN's English-Manipuri Dictionary and SAGOLSEM's Pukeilol.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`. Note Microsoft
translates a bare "&" to অমসুং (amasung) in running text but keeps it in tags, placeholders, and shortcuts; our
placeholder tokens are protected by the same check.

## Plurals

`new Intl.PluralRules('mni').resolvedOptions().pluralCategories` reports `one` / `other` (two branches, like English).
Manipuri marks plurality with a suffix (-shing / -singh) rather than agreement, and there are no gender-specific forms,
so the grammar interacting with counts is light. Verify the categories the runtime actually returns for the exact tag
you ship (`mni`, `mni-Mtei`, or `mni-Beng`) before writing branches, since CLDR data for the script variants can differ.

## Decision points

These are the calls that actually matter for Manipuri; settle them before a translation pass.

- **Viability / priority (the headline finding).** Manipuri is a low-resource language for software localization with
  almost no shipped major-vendor UI to copy. macOS, Windows, Spotify, and Netflix do NOT ship a Manipuri interface in
  either script; what exists is input-method/keyboard support (Apple, Google, Microsoft SwiftKey) and machine
  translation, not localized product UI. The only real localization precedent in the reference pile is a Microsoft
  _translation style guide_ (privacy-content register), not a shipped OS. Practical effect: tiny addressable user base,
  no app-UI vocabulary to inherit, and the translator coins or borrows most of it. Recommendation: low priority; ship
  only with a specific reason and a native reviewer. This is a David-only call. Confidence: high.

- **Script: Meitei Mayek vs Bengali-Assamese (the decision that shapes everything).** Manipuri is written in two scripts
  and the major precedents genuinely disagree:
  - Meitei Mayek (ꯃꯤꯇꯩ ꯃꯌꯦꯛ, an indigenous abugida, tag `mni-Mtei`) is the official script of Manipur, taught in schools
    since 2006, and Meitei-language newspapers switched to it in January 2023. Google Translate and the Government of
    India's Bhashini both use Meitei Mayek. It's the culturally and officially ascendant choice.
  - Bengali-Assamese / Eastern Nagari (tag `mni-Beng`) is the historically dominant script and still in wide everyday
    use. Microsoft's 2025 Manipuri style guide is written entirely in Bengali script, so the one shipped-vendor
    precedent picks Bengali.
  - Rendering: Noto Sans Meetei Mayek (free, SIL OFL, Unicode block U+ABC0–ABFF) renders the script well, so a bundled
    webfont closes most of the gap. But out-of-the-box OS/system-font fallback for Meitei Mayek is thinner than the
    ubiquitous Bengali fonts, so Bengali "just renders" on more machines today while Meitei Mayek may need Cmdr to ship
    the font.
  - Recommendation: if Cmdr ships Manipuri at all, prefer Meitei Mayek (`mni-Mtei`) to align with the official
    direction, schools, and the younger user base, and bundle Noto Sans Meetei Mayek so rendering doesn't depend on the
    OS. Choose Bengali (`mni-Beng`) only if the target users skew older/diaspora or if avoiding a bundled font matters
    more than script alignment. This is a David-only call. Confidence: low (it's a values-and-audience call, not a
    technical one), and it determines the catalog dir tag and the per-string transliteration.

- **Anglicism / tech-term borrowing.** Manipuri borrows computing terms from English heavily rather than coining native
  words; the Microsoft examples transliterate file-manager-adjacent vocabulary directly. Expect "file", "folder", "tab",
  "volume" and similar to become transliterations unless a clear, widely-understood native word exists. Be consistent,
  record each call in the glossary, and have a native reviewer confirm the transliteration spelling (the same word can
  be spelled several ways). Confidence: high that borrowing is the norm; low on individual spellings.

- **Punctuation, numbers, and dates (Microsoft conventions to reuse).** No capitalization exists in either script (a
  single form per letter). The Nuqta diacritic is strictly avoided. Spell out numbers one through ten, numerals above.
  Microsoft uses DD/MM/YYYY and "Month YYYY"; that conflicts with Cmdr's house ISO-date default, so flag the date-format
  choice for David rather than silently following either. No gender-specific words, so no count/gender agreement to
  manage. Confidence: high on the script conventions, open on the date format.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/mni/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
