# Kyrgyz (ky) translation style guide

Working notes for translating Cmdr into Kyrgyz. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Kyrgyz.

## Voice and tone

Friendly, concise, active, calm. Address the user politely (see Formality). Error messages stay calm and actionable.

## Formality

Formal/polite address (`Сиз`, the V-form), confirmed from the GNOME catalog: the polite -ңыз/-ңиз endings and `Сиз`
appear throughout, with zero informal `Сен`. Use the polite register consistently.

## Decision points

Script, Cyrillic (David should ratify):

- Options: Cyrillic vs Latin. Cyrillic is the sole official script in Kyrgyzstan and both reference sources (GNOME,
  Microsoft terminology) are Cyrillic, confirmed from the actual strings (with Kyrgyz letters Ө/Ү/Ң). The Latin debate
  is real but dormant: in 2023 the president called a Latin switch premature; Kyrgyzstan abstained from the 2024 Turkic
  common-Latin alphabet with no timeline. No production system uses Latin.
- Recommendation: Cyrillic only; don't build Latin. Confidence: high.

Major-product localization is essentially absent (THE finding):

- Apple: Kyrgyz is a keyboard/region option only, NOT a macOS UI display language, no Finder localization (hence no
  Tier-1 source). Microsoft: a terminology glossary exists, but Kyrgyz isn't a standard Windows display language.
  Google/Spotify/Netflix: no Kyrgyz UI found. There is no authoritative full-UI prior art to anchor expectations.
- Recommendation: Cmdr would be a near-first-mover. Term choices lean on Microsoft terminology + GNOME, both partial and
  old (the Nautilus catalog is ~42% translated, dated 2012). Flag for David: is shipping Kyrgyz worth it given near-zero
  major-vendor precedent and a thin reference base? Confidence: high that precedent is absent; the go/no-go is David's.

Placeholder + case-suffix agreement (engineering pitfall):

- Kyrgyz is agglutinative with strict vowel harmony: case suffixes (6 cases) must harmonize with the final vowel of the
  preceding word (front/back, rounded/unrounded), plus consonant assimilation. A suffix attached to or following a
  `{name}` filename can't agree at translation time.
- How the source copes: GNOME puts the suffix on the fixed noun, not the variable ("«%s» папкасын", suffix on папка,
  sidestepping the placeholder).
- Recommendation: never glue a grammatical suffix directly to a `{placeholder}`; structure messages so suffixes land on
  fixed words (the «%s» + fixed-noun+suffix pattern), or keep the inserted value in nominative at the sentence end. The
  exact rewrite per string is a native-review task. Confidence: high (grammar well-documented).

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Sources are thin (GNOME ~42%, MS terminology), so confidence is
modest; native review needed.

- file → файл · MS · high
- folder → папка · MS, GNOME · high
- copy → көчүрүү · MS · high
- search → издөө · MS, GNOME · high
- open → ачуу · GNOME · high
- delete → өчүрүү · GNOME · high
- rename → атын өзгөртүү · GNOME · high
- trash → Себет · GNOME (MS: Таштанды кутусу) · tentative, Себет is shorter and matches the file-manager domain
- cancel → (button) tentative · GNOME uses "Калтыруу"; MS's "тандоону чечүү" is the _deselect_ sense, not a dialog
  Cancel, don't use it for the button

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

## Plurals

CLDR categories: `one`, `other`. Note: GNOME's old catalog declared `nplurals=1` (gettext convention), but Cmdr's intl
layer is CLDR-based, so author both `one` and `other`. Kyrgyz typically keeps the noun singular after numerals, so the
two forms are often the same word, but the framework still needs both buckets. The `desktop-i18n-plural` check requires
every plural message to cover the categories this language needs.

## Notes and decisions

- Cyrillic with Kyrgyz-specific letters: Ө ө, Ү ү, Ң ң. Don't substitute Russian look-alikes.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- Go/no-go on shipping Kyrgyz at all (no major-vendor precedent, thin/old reference base).
- Trash term (Себет vs Таштанды кутусу) and the Cancel-button term.
- Every placeholder-bearing string needs native review for suffix agreement.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in `docs/guides/i18n-translation.md` and
`apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ky/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
