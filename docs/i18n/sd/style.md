# Sindhi (sd) translation style guide

Working notes for translating Cmdr into Sindhi. Read [`README.md`](../README.md) for how this fits the translation
process.

Low resourcing AND a hard script split. The pile has Microsoft terminology for `sd` (Perso-Arabic) and Microsoft style
guide for `sd-Deva` (Devanagari); NO macOS Finder. Apple does not ship a Sindhi macOS UI. The script choice below is the
defining decision and a real fork in the road.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. With no macOS reference, prioritize plain,
clear Sindhi. Error messages stay calm and actionable. Treat all terms as tentative pending native review.

## Formality

Sindhi has familiar/polite address distinctions; software convention leans polite/neutral. Use the imperative/command
form for actions and polite address in running text. Microsoft's sd terminology is the register reference. Confidence:
tentative.

## Decision points

### Script: Perso-Arabic vs Devanagari (the defining choice)

Sindhi is written in TWO scripts, and they're not interchangeable spellings, they're different writing systems with a
different reading direction:

- **Perso-Arabic (extended, RTL)**: the sole official script in Pakistan (Sindh province), where the large majority of
  Sindhi speakers live. Microsoft's `sd` terminology uses this.
- **Devanagari (LTR)**: used by Sindhi communities in India. Microsoft's `sd-Deva` style guide uses this.
- BCP-47: `sd` (or `sd-Arab`) = Perso-Arabic; `sd-Deva` = Devanagari. These are SEPARATE locales, never one blended
  text.
- Apple ships no Sindhi macOS UI at all. Google/Microsoft treat them as distinct locales.

The two are abjad (Perso-Arabic) vs abugida (Devanagari) and one is RTL while the other is LTR, so this isn't just a
font swap; it changes the entire layout-direction story for the app.

Recommendation: if Sindhi ships at all, target **Perso-Arabic `sd` (Pakistan)** as the primary, since it's the official
script and the larger speaker base; treat `sd-Deva` as a separate, lower-priority LTR variant only if Indian-Sindhi
demand appears. Confidence: high on Perso-Arabic being the primary; the whole language is low priority though. Flag for
David: confirm Sindhi is in scope, and if so that `sd` base = Perso-Arabic. The Perso-Arabic primary is RTL, so it
inherits the entire RTL-readiness gate (see below).

### RTL (for the Perso-Arabic primary)

Perso-Arabic Sindhi is right-to-left. Everything in the Pashto guide's RTL section applies identically here: the app
must mirror panes, navigation arrows, chevrons, and progress; LTR runs (paths, brand names, numbers) need bidi
isolation. The two-pane left/right model mirrors. Recommendation: don't ship Perso-Arabic Sindhi until the app's RTL
mirroring is verified. (Devanagari Sindhi, by contrast, is LTR and avoids this.) Confidence: high.

### Numerals

Perso-Arabic Sindhi may use Eastern Arabic-Indic numerals; Devanagari Sindhi may use Devanagari digits. Let `Intl` with
the locale tag shape numerals; confirm audience expectation with a native reviewer. Confidence: tentative.

### Gender and inclusive language

Sindhi is grammatically gendered with verb agreement. Avoid making the user the gendered subject; structure around
imperatives and nouns. No established neutral convention in Sindhi UI. Recommendation: rephrase to avoid user-gender.
Confidence: tentative.

## Terminology and glossary

Defer; no macOS source, so every term is tentative and needs native review. Triangulate Microsoft sd terminology
(Perso-Arabic) and sd-Deva style guide only, and keep separate columns per script if both ship.

| English term | Sindhi (Perso-Arabic) | Notes                         |
| ------------ | --------------------- | ----------------------------- |
| file         | فائل                  | confirm with reviewer         |
| folder       | فولڊر                 | tentative                     |
| trash        | ردي                   | tentative, needs native check |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`. In Perso-Arabic (RTL) these LTR brand runs need bidi isolation.

## Plurals

CLDR categories for `sd`: `one`, `other`. Only two plural forms needed (same for sd-Deva).

## Notes and decisions

- Sindhi is LOW priority for launch absent specific demand: no Apple OS reference, a two-script fork, and the
  Perso-Arabic primary drags in the RTL workstream. Recommend deprioritizing until an RTL workstream and native reviewer
  exist.

## Decisions to confirm with David

- Is Sindhi in scope at all? If yes, `sd` base = Perso-Arabic (Pakistan, RTL), with `sd-Deva` (Devanagari, LTR) as a
  separate later variant?
- RTL readiness gates the Perso-Arabic primary (shared with Pashto and other RTL locales).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/sd/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
