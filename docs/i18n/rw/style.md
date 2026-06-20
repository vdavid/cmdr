# Kinyarwanda (rw) translation style guide

Working notes for translating Cmdr into Kinyarwanda. Read [`README.md`](../README.md) for how this fits the translation
process.

LOW priority. Kinyarwanda is the national language of Rwanda (~12M speakers, also widely understood in the region). The
pile has GNOME Nautilus (`rw`) and Microsoft terminology (`rw-RW`); NO macOS (Apple ships no Kinyarwanda UI). So a real
file-manager catalog (Nautilus) exists, which helps, but the highest-authority OS reference is absent.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. Lean on GNOME Nautilus for file-manager term
precedent and Microsoft for general software register. Native review required for confidence.

## Formality

Latin script. Kinyarwanda has politeness registers but software convention is plain/neutral. Use imperative for actions.
Confidence: tentative.

## Decision points

### Resourcing / scope

The headline call is whether to localize. No Apple OS support; the assets are Nautilus (file-manager-relevant, GPL,
reference-only) + Microsoft terminology. Recommendation: low priority for launch absent specific Rwandan-market demand;
if pursued, Nautilus is the best file-manager term source. Confidence: high that it's low priority.

### Noun-class agreement (the real grammar trap)

Kinyarwanda is a Bantu language with an extensive noun-class system (16+ classes). Agreement (verb prefixes, adjectives,
demonstratives) is driven by the noun's class, and crucially **plural agreement and number words depend on the noun
class**, not a simple singular/plural like English.

- This means a count message ("{count} files") needs the verb/agreement to match the noun class of "file", ICU plural
  one/other can't capture noun-class agreement on its own.
- Inserted `{path}`/`{name}` values that are foreign words have unpredictable class, another reason to isolate inserts in
  their own slot rather than agreeing with them.
- Recommendation: phrase count and possessive messages so agreement is fixed by the in-message noun (which you control),
  never by an insert; have a native speaker verify class agreement. Confidence: high that this is the key hazard.

### Numerals

Western digits; `Intl` handles formatting. Confidence: high.

### Gender

Kinyarwanda has NO grammatical gender (Bantu noun classes are not gender), so the gender-agreement problems that plague
European/Slavic/Semitic translations don't apply. This is one area that's actually simpler. Confidence: high.

## Terminology and glossary

Defer; triangulate GNOME Nautilus (file-manager terms) + Microsoft rw-RW terminology. Native review needed.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `rw`: `one`, `other`. Two CLDR forms, but note the noun-class agreement caveat above means
"one/other" doesn't fully describe Kinyarwanda number grammar; the branches still need correct class agreement baked in.

## Decisions to confirm with David

- Is Kinyarwanda in scope? Recommend low priority for launch (no Apple reference, needs native reviewer). The
  noun-class agreement on counts is the technical gotcha to brief any translator on.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/rw/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
