# Lingala (ln) translation style guide

Working notes for translating Cmdr into Lingala. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Lingala.

Low-priority signal is strong (see Decision points). The only reference is one decade-old community GNOME Nautilus
catalog (`ln/gnome-nautilus/nautilus.po`, 2016, single Ubuntu volunteer, ~852/997 translated). No macOS (Tier 1), no
Microsoft (Tier 2). Notably, this catalog uses full scientific orthography WITH tone marks.

## Voice and tone

Friendly, concise, active, calm, if ln is built at all. Error messages stay calm and actionable.

## Formality

Minimal T-V distinction; respect is expressed via the plural address. The catalog doesn't establish a strong formal/
informal split. Keep a neutral, respectful register; defer fine calls to a native reviewer.

## Decision points

Ship ln at all, or fall back to French? (strategic call for David):

- Lingala is a Bantu lingua franca of DR Congo and Republic of Congo (tens of millions of speakers), an official
  national language, but NOT a computing-UI language. French dominates computing across both Congos. No major vendor
  (Apple, Microsoft, Google products, Spotify, Netflix) localizes into ln.
- Recommendation: French fallback is the pragmatic default for the region; an ln localization would be a
  near-first-mover with essentially zero vendor reference and a thin, aged community corpus. Low priority. Confidence:
  high. Flag for David.

Orthography / diacritics (ɛ ɔ + tone marks):

- Lingala uses Latin script with seven vowels; ɛ and ɔ are distinct letters in the standard/scientific orthography. Tone
  is phonemic (high marked with acute, low unmarked), but tone marking is inconsistent in everyday writing. There's no
  single enforced standard.
- The only file-manager corpus (Nautilus) uses full ɛ/ɔ + tone marks (e.g. `fisyé ya sistɛ́mɛ`, `sɛgɔ́ndi`, `dosíye`).
- Recommendation: if shipping ln, match the Nautilus precedent (full ɛ/ɔ + tone marks), it's the only authority, and
  these are proper Unicode letters. Confirm the app font covers ɛ/ɔ with combining acute. Confidence: medium-high.

Noun-class / count agreement (the {count}/{placeholder} pitfall):

- Bantu noun classes mean the plural is a distinct word form (often a `Ba`-prefixed particle) plus word reordering, not
  a suffix. The catalog shows this: `%'u folders` → `[0] %'u Dosíye` / `[1] Ba dosíye %'u`, the placeholder/word order
  shifts between forms. A naive `{count} {noun}s` template will be wrong, and agreement varies by the noun's class.
- Recommendation: use full CLDR plural messages (one/other) with the WHOLE noun phrase translated per form, never string
  concatenation. Multi-noun counted strings can't be fully covered by count plurals alone, flag them for a native
  reviewer. Confidence: medium.

## Terminology and glossary

From the 2016 Nautilus catalog (loose reference only; many French loanwords). Format:
`English → chosen · source · confidence`.

- file → fisyé (from French fichier; pl. Ba fisyé) · GNOME · tentative
- folder → dosíye / dosyé (from French dossier; pl. Ba dosíye) · GNOME · tentative
- copy → Kopi (loanword) · GNOME · tentative
- open → Fungola · GNOME · tentative
- cancel → Koboma · GNOME · tentative
- rename → Batisa · GNOME · tentative
- trash → Ekundé · GNOME · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other`, where `one` covers BOTH 0 and 1 (rule i = 0,1) and `other` is everything else. (The
GNOME header uses the older gettext `plural=(n>1)`, which groups 0+1 the same way; CLDR is authoritative for Cmdr's intl
stack.) The `desktop-i18n-plural` check requires every plural message to cover the categories this language needs.

## Notes and decisions

- Special letters: ɛ, ɔ (open vowels), plus acute/circumflex/caron tone marks. Don't ASCII-fold them if matching the
  Nautilus precedent.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- The big one: ship ln at all, or fall back to French for the region? (Recommendation: not a priority; French fallback.)
- If shipping: confirm full ɛ/ɔ + tone-mark orthography, and budget a native reviewer for noun-class count agreement.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in
[`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ln/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
