# Sanskrit (sa) translation style guide

Working notes for translating Cmdr into Sanskrit. Read [`README.md`](README.md) for how this fits the translation
process.

VERY LOW priority. Sanskrit is a classical/liturgical language of India with very few daily-use speakers; it's localized
mostly for symbolic/heritage reasons, not market reach. The pile has ONLY a Microsoft style guide for `sa`; no macOS
(Apple ships no Sanskrit UI), no GNOME/Xfce, no terminology glossary. Essentially no consumer software-localization
precedent.

## Voice and tone

Friendly, concise, active, never alarmist in principle, but Sanskrit lacks an established casual-software register.
Microsoft's style guide is the only anchor. Any output needs review by someone fluent in Sanskrit. Confidence: tentative
across the board.

## Formality

Devanagari script (the standard for modern Sanskrit; though historically script-agnostic, Devanagari is the de facto
software script). Sanskrit has rich honorific registers; software convention is essentially undefined. Use a neutral
respectful register. Confidence: tentative.

## Decision points

### Resourcing / scope (the headline finding)

The real decision is whether to localize at all. No Apple OS support, no file-manager catalogs, only a Microsoft style
guide and no terminology glossary. Recommendation: DEPRIORITIZE strongly; Sanskrit is a heritage/symbolic locale, not a
reach play, and needs a specialist reviewer. Confidence: high that this is lowest-priority.

### Script: Devanagari

Modern Sanskrit software uses Devanagari (LTR). No script split to settle in practice. Recommendation: Devanagari.
Confidence: high.

### Coining technical vocabulary (the hard part if pursued)

Sanskrit has no native vocabulary for "file", "folder", "tab", "pane", etc. Sanskrit localizations typically COIN terms
from Sanskrit roots (e.g. संचिका for file, सूची-type constructs), and different efforts coin differently, there's no
settled standard. With no Microsoft terminology glossary and no Nautilus catalog in the pile, every tech term would be a
fresh coinage needing expert agreement.

- Recommendation: do NOT attempt without a Sanskrit-computing specialist; the terminology doesn't exist off-the-shelf.
  Confidence: high that this is the blocking difficulty.

### Numerals

Western (or Devanagari) digits; `Intl` with `sa` handles shaping. Confidence: high.

### Sandhi and compounding

Sanskrit's sandhi (sound-change at word boundaries) and heavy compounding mean concatenating a `{placeholder}` into a
sentence can produce grammatically wrong joins. Recommendation: isolate inserts in their own slot (e.g. label-then-value)
rather than inlining them into a sandhi context. Confidence: high that this is a real hazard.

## Terminology and glossary

None available in the pile beyond the style guide; all terms would be fresh coinages. Defer entirely to a specialist.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `sa`: `one`, `other`. (Sanskrit grammatically has three numbers, singular, dual, plural, but CLDR
collapses selection to one/other for `sa`.)

## Decisions to confirm with David

- Recommend NOT in scope for launch. Sanskrit is symbolic, has no off-the-shelf tech terminology, and needs a specialist
  to coin every term. Only pursue if there's a deliberate heritage reason and a committed Sanskrit-computing reviewer.
