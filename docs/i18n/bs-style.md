# Bosnian (bs) translation style guide

Working notes for translating Cmdr into Bosnian. Read [`README.md`](README.md) for how this fits the translation
process.

`bs` is the language base, written in the Latin script (see Decision points for why Latin, not Cyrillic). The reference
pile has GNOME nautilus for `bs`, plus Microsoft terminology/style-guide material under the script-tagged siblings
`bs-Latn` and `bs-Cyrl`.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Match the English register; keep error and crash copy reassuring and
factual, never using the bare labels "error"/"failed".

## Formality

**Address the user with the formal `Vi` (vykanje), recommended.** Bosnian (like Serbian and Croatian) distinguishes
informal `ti` from formal/polite `Vi`. Software UI in the South Slavic family conventionally uses the polite `Vi` form
for addressing the user, capitalized as `Vi`. Microsoft's Bosnian (Latin) localization follows this. Confidence: high
that `Vi` is the conventional choice; a native reviewer should confirm Cmdr's friendly voice doesn't warrant `ti`
instead (some modern consumer apps lean informal).

**Imperatives for UI actions**: use the form consistent with the address choice; the `Vi`-register imperative for
buttons and menu items (e.g. "Kopiraj" stays a bare imperative regardless, but full-sentence prompts take the `Vi`
verb forms).

## Decision points

The defining Bosnian decision is script.

- **Script: Latin (`bs` base), settled.** Bosnian is officially biscriptal (Latin and Cyrillic are both constitutional),
  but Latin overwhelmingly dominates everyday life, media, and software. The script-tagged siblings exist in the
  reference pile (`bs-Latn`, `bs-Cyrl`), confirming both are localizable, but:
  - Latin is the default and dominant script in modern Bosnian usage (verified via web research, 2026-06-20).
  - Microsoft ships Bosnian primarily in Latin (`bs-Latn` is the maintained MS locale; `bs-Cyrl` exists but is far less
    used).
  - Recommendation: target Latin for the `bs` base. Only add a `bs-Cyrl` variant if a real Cyrillic-preferring audience
    surfaces, which is unlikely for a macOS app. Confidence: high. No David call needed beyond confirming Cmdr won't
    pursue a Cyrillic variant.
- **Mutual intelligibility with Serbian/Croatian.** Bosnian, Croatian (`hr`), Serbian (`sr`), and Montenegrin are
  mutually intelligible variants of one diasystem (BCMS). A Bosnian user reads Croatian/Serbian-Latin near-perfectly.
  This is informational, not a decision: don't conflate the catalogs, but expect heavy term overlap. Bosnian's
  distinguishing features are some Turkisms and the optional `h` in certain words (e.g. "kahva" vs "kafa"); a native
  reviewer pins these. Confidence: high.
- **No grammatical-gender trap for the user, but adjective/participle agreement matters.** Bosnian doesn't gender the
  address pronoun, but past-tense verbs and adjectives agree with the subject's gender and number. Phrasing that
  describes the user's action ("you moved", "you deleted") must avoid assuming the user's gender; prefer neutral
  framing. Confidence: high; a translator-craft concern.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Sources for `bs`: GNOME (Tier 3) and Microsoft terminology
via `bs-Latn` (Tier 2).

- file → datoteka · MS / GNOME · high
- folder → folder / direktorij · MS (varies) · tentative, confirm preferred form with native reviewer
- copy → Kopiraj · GNOME / MS · high
- delete → Izbriši · GNOME / MS · high
- trash → Smeće / Korpa · GNOME · tentative, "Smeće" (rubbish) vs "Korpa" (basket); native reviewer to pick
- cancel → Otkaži · MS · high
- search → Pretraži · MS / GNOME · high
- settings → Postavke · MS / GNOME · high

Populate fully from `bs/gnome-nautilus/` and `bs-Latn/microsoft-terminology/` during translation; the table above is a
starting point, not exhaustive.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: `one`, `few`, `other` (verified with `new Intl.PluralRules('bs')`, 2026-06-20). Three branches, the
standard South Slavic system: `one` for n ending in 1 (but not 11), `few` for n ending in 2–4 (but not 12–14), `other`
for the rest. Every counted string needs all three; the noun case after the numeral differs by branch (nominative
singular after `one`, genitive singular after `few`, genitive plural after `other`), so write each branch with the
correct case form, not just a swapped number.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Bosnian uses a comma decimal separator and a period (or space)
  for thousands; let the formatter decide, don't hardcode.
- **Quotation marks.** Bosnian print uses `„…"` (low-high) or `»…«`; UI catalogs often use plain curly. Match the
  surrounding source and stay consistent.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: [`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Decisions to confirm with David

- **Formality: `Vi` (formal) recommended, but Cmdr's friendly voice might prefer `ti` (informal).** A native reviewer
  should settle which fits Cmdr's tone; the South Slavic software convention is `Vi`.
- **Trash and folder terms** have competing options ("Smeće"/"Korpa", "folder"/"direktorij") needing a native call.
