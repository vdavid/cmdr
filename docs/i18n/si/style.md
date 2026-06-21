# Sinhala (si) translation style guide

Working notes for translating Cmdr into Sinhala (Sri Lanka). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Sinhala.

## Decisions to confirm with David

- **Priority: low; defer until there's clear user demand (high).** This is the headline finding, not a footnote. **Apple
  does not localize macOS or iOS into Sinhala** (it ships a Sinhala keyboard and Unicode rendering, but the system UI
  and language picker have no Sinhala), so there is **no OS-native Sinhala UI for a macOS app to match or borrow
  conventions from**. Netflix and Spotify also skip Sinhala. Only Microsoft (full Windows/Office localization, with a
  published Sinhala style guide and a ~2,000-word ICTA/Microsoft glossary) and Google (web UI) localize into it
  meaningfully. For an indie macOS app, this is a deprioritize-and-wait signal. Flag for David: is Sinhala worth a pass
  at all yet, given no macOS precedent and a thin reference pile?
- **Reference-pile coverage is thin (note).** The local pile has only GNOME Nautilus and Xfce Thunar for Sinhala (Tier
  3): **no macOS, no Microsoft terminology or style guide.** So most term choices here can only reach `tentative` until
  someone pulls Microsoft's Sinhala glossary/style guide or a native reviewer signs off. The macOS-wins rule that
  anchors the European guides doesn't apply: there's no macOS Sinhala to win.

## Voice and tone

Friendly, concise, calm, in the **formal written/literary register** (see Decision points). Sinhala is strongly
diglossic, and software uses the written register, so the warmth comes from polite, clear phrasing rather than from
colloquial spoken Sinhala (which would read oddly in an app). Error messages stay calm and actionable: phrase the
problem and the next step, in the polite written form, and don't use a bare "error"/"failed"-style status label.

## Formality

- **Use the formal written/literary register with polite request forms for actions.** This is the established software
  convention (Microsoft's Windows/Office Sinhala and its style guide are written-register) and the South Asian UI norm.
  A naive translator may drift toward colloquial spoken Sinhala; name this convention explicitly so they don't.
- **Imperatives/commands: polite request forms, not bare colloquial imperatives.** GNOME Sinhala uses the polite `-න්න`
  request form (for example "අවලංගු කරන්න" for Cancel). Prefer that courteous request form on buttons and menu actions.
  (verified against the reference pile, 2026-06-20: GNOME Nautilus Sinhala uses "අවලංගු කරන්න" for Cancel.)

## Decision points

- **Script: Sinhala abugida only, no Latin/"Singlish" variant (high).** Sinhala is written in the Sinhala (Brahmic)
  abugida. **Singlish** (romanized Sinhala) exists only as an input/transliteration method and an informal social-media
  convention, never as a shipped product UI locale. No major ships a Latin-script Sinhala UI, so the UI script is
  unambiguously Sinhala script.
- **Complex-script rendering and truncation (high).** Sinhala is a complex script with OpenType shaping: consonant
  conjuncts, the al-lakuna (virama), and vowel signs (matras) that attach above, below, before, or after the base
  consonant, including pre-base reordered vowels. Modern macOS CoreText / the WebView and current fonts (Noto Sans
  Sinhala, Iskoola Pota) render it correctly, so a Tauri app on current macOS is fine. **Gotcha for the UI:** never
  truncate by code-point or character count or break mid-cluster, since you can split a vowel sign from its base or
  break a conjunct and produce a dotted-circle or garbage. **Truncate on grapheme-cluster boundaries** (prefer CSS
  ellipsis-at-end over manual string slicing). Stacked conjuncts plus above/below matras make Sinhala **taller per
  line** than Latin, so give rows vertical headroom or glyphs clip.
- **Regional variant: one, `si` (`si-LK`).** Spoken essentially only in Sri Lanka. No variant matrix. Confidence: high.
- **Capitalization: not a concept (high).** The Sinhala script has **no letter case** (no upper/lower distinction). The
  app's sentence-case rule is simply moot for Sinhala strings; don't try to apply title-vs-sentence case.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. The only pile sources for Sinhala are GNOME Nautilus and Xfce Thunar
(Tier 3); there's no macOS or Microsoft data locally, so confidence caps at `tentative` here. Pull Microsoft's Sinhala
glossary/style guide or get a native reviewer before promoting any of these. Evidence verified against the reference
pile (`_ignored/i18n/si/`) on 2026-06-20; sources decide the term, Cmdr writes its own value (GNOME/Xfce GPL, never
copied verbatim).

Seed terms (GNOME-backed, all `tentative`):

- **folder: `බහලුම`** · GNOME Nautilus. `tentative`.
- **trash: `ඉවතලන බඳුන`** · GNOME Nautilus. `tentative`.
- **cancel: `අවලංගු කරන්න`** · GNOME Nautilus (polite request form). `tentative`.

Add file, copy, move, eject, server, volume, pane, tab, sort, bookmark, search, settings as the translation pass reaches
them, ideally cross-referenced against Microsoft's Sinhala glossary, not GNOME alone.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Since macOS isn't localized into Sinhala, the macOS UI names Cmdr opens into
will appear to the user in English (or the user's actual macOS language), not Sinhala, so keep those references matching
what the user's system actually shows.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('si')`; the GNOME catalog header is `nplurals=2`).
Write both branches.

- **Don't auto-pluralize off the singular.** Sinhala plural morphology correlates with **animacy** and is often
  _subtractive_ for inanimate nouns: the plural can be **shorter** than the singular (counter-iconic), the opposite of
  English "+s". Animate nouns take suffixes like `-වරු`/`-යන්`. So the translator must supply both forms explicitly;
  never derive the plural mechanically. The two-bucket `one`/`other` structure still holds for message authoring.
- The `desktop-i18n-plural` check requires every plural message to cover both categories.

## Notes and decisions

- **Encoding check to add when translating (TODO):** before the `si` catalog ships, wire a ZWJ-for-ligatures consistency
  guard (a milder version of Malayalam's chillu check). Sinhala uses ZWJ (U+200D) to form certain conjunct ligatures, so
  the same word can be encoded with or without ZWJ and look near-identical while differing in bytes, which breaks
  search, sort, and dedup. The check should enforce a single consistent ZWJ convention across the `si` catalog (don't
  mix ZWJ and non-ZWJ forms of the same cluster) so string compares stay stable. Build this check when `si` is actually
  translated.
- **Capitalization is moot** (no case in the script); see Decision points.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` produce locale-correct output;
  never hardcode separators. Sinhala uses Western (Arabic) digits in modern UI.
- **Length / overflow.** Sinhala runs comparable to or modestly longer than English horizontally, but notably **taller
  per line** (stacked conjuncts, above/below matras). The real risk is vertical clipping and mid-cluster truncation, not
  German-style horizontal blowout. Give labels normal expansion headroom, ensure line/row heights have vertical slack,
  and truncate on grapheme clusters. Overflow-check against the pseudolocale (`en-XA`) plus a real Sinhala sample.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/si/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
