# Māori (mi) translation style guide

Working notes for translating Cmdr into te reo Māori. Read [`README.md`](README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into Māori.

Unusually for a low-resource language, te reo Māori has strong real localization: Microsoft ships a Windows/Office
Language Interface Pack plus a full style guide and a maintained terminology base, all built in accordance with Te Taura
Whiri i te Reo Māori (the Māori Language Commission). Sources for `mi`/`mi-NZ`: the Microsoft terminology TBX (rich) and
style guide PDF (the lead reference), plus an old, ~20%-complete GNOME Nautilus catalog. Apple does NOT ship macOS in
Māori, so Microsoft fills the Tier-1 role here. We write to base tag `mi`.

## Voice and tone

Friendly, concise, active, calm, respectful. Microsoft's Māori voice is "warm and relaxed, less formal," which fits
Cmdr's voice well. Error messages stay calm and actionable.

## Formality

No T-V (formal/informal "you") distinction. Use the neutral possessive forms (taku, tō, tana) unless a more formal tone
is called for (MS guidance). Register is respectful but conversational.

## Decision points

Macrons, always use them, never double-vowel:
- Modern standard te reo marks long vowels with macrons: ā ē ī ō ū. Te Taura Whiri has preferred macrons since its 1987
  founding, and Microsoft uses them throughout (including in possessives: `ā rātou mahi` vs personal `a rātou`).
- Options: macrons / double-vowel ("aa") / no diacritics. Sole carve-out: personal/family/hapū/iwi names where the owner
  prefers double-vowel spelling, irrelevant to UI strings.
- Recommendation: macrons, mandatory, including in possessives. macOS renders combining macrons cleanly. Confidence:
  high.

Use official Te Taura Whiri / Microsoft coined native terms, not transliteration:
- Te Taura Whiri coins native tech terms (rorohiko = computer, kōnae = file, kōpaki = folder), and the Microsoft pack
  uses them consistently. The old GNOME catalog diverges on several (open, delete, cancel, Trash) and is only ~20%
  complete.
- Recommendation: adopt the Microsoft/Te Taura Whiri terms verbatim as Cmdr's term choices (write our own strings, don't
  paste their copyrighted UI strings). When GNOME and Microsoft conflict, go Microsoft. Confidence: high.

Possessive a/o categories (a real grammar trap, native review needed):
- Māori distinguishes a-category vs o-category possession, with macron-on-possessive rules. Any string with possession
  ("your files", "its name") forces a choice; the MS guide defers to Harlow's Māori Reference Grammar.
- Recommendation: don't let an agent guess a/o, flag any string with a possessive for a native reviewer. Confidence:
  high that it's a real subtlety; per-string resolution needs a human.

## Terminology and glossary

From Microsoft mi-NZ terminology (Tier 2, the lead source; GNOME differs and is weak corroboration only). Format:
`English → chosen · source · confidence`.

- file → kōnae · MS · high
- folder → kōpaki · MS · high
- copy → tārua · MS · high
- cut → tapahi · MS · high
- paste → whakapiri · MS · high
- move → nuku · MS · high
- delete → Muku · MS (GNOME: Porowhiu, prefer MS) · high
- open → whakatuwhera · MS (GNOME: Huaki, prefer MS) · high
- save → tiaki · MS · high
- close → kati · MS · high
- send → tuku · MS · high
- search → rapu · MS · high
- settings → Ngā Tautuhinga · MS · high
- cancel → wetetīpako · MS (GNOME: Whakakore, a simpler common alternative) · tentative
- trash / recycle bin → Ipu Para · MS (GNOME Trash: Te Para) · high
- computer → rorohiko · MS · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other`. Māori's dual/plural distinctions live in pronouns (tāua/mātou…), not in noun counting,
so message pluralization needs only one/other. The `desktop-i18n-plural` check requires every plural message to cover the
categories this language needs.

## Notes and decisions

- Macrons everywhere (ā ē ī ō ū); never double-vowel in UI strings.
- Cmdr's English voice avoids the word "error", Māori `hapa` exists but the copy rule still applies.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- cancel → wetetīpako (MS) vs Whakakore (simpler, GNOME): confirm which reads better as a button.
- Any string with a possessive ("your", "its") needs native a/o review before shipping.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in [`../guides/i18n-translation.md`](../guides/i18n-translation.md)
and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
