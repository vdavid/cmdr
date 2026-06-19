# Tajik (tg) translation style guide

Working notes for translating Cmdr into Tajik. Read [`README.md`](README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into Tajik.

Base tag `tg` means Tajik in CYRILLIC script (the official, default script). See Decision points.

## Voice and tone

Friendly, concise, active, calm. Address the user politely (see Formality). Error messages stay calm and actionable, and
avoid alarmist wording, matching Cmdr's English register.

## Formality

Polite/formal address. Tajik (like Persian) has a T-V distinction: informal `ту` vs respectful `шумо`. Software uses
`шумо` consistently. Confirmed from the GNOME catalog: 56 occurrences of `шумо`/`Шумо` and zero informal `ту` in
sentence contexts. Use `шумо` throughout.

Imperatives in UI actions: Tajik, like Persian, names menu actions and buttons as verbal nouns (the masdar / infinitive
form), not bare imperatives. The reference catalogs do this consistently: `_Copy` → `Нусха бардоштан`, `_Delete` →
`Нест кардан`, `_Paste` → `Гузоштан`, `Open` → `Кушодан`, `Cancel` → `Бекор кардан`. Follow the verbal-noun convention
for button and menu labels.

## Decision points

Script, Cyrillic (David should ratify):
- Options: Cyrillic vs Perso-Arabic vs Latin. Cyrillic is the sole official script in Tajikistan and the only script
  used in production systems, schools, government, and media. Both reference sources (GNOME, Microsoft terminology) are
  Cyrillic. Use the Tajik-specific extra letters: `ғ ӣ қ ӯ ҳ ҷ` (and capitals `Ғ Ӣ Қ Ӯ Ҳ Ҷ`); don't substitute Russian
  look-alikes.
- NOT Perso-Arabic: that script is for Persian as written in Iran and for Dari in Afghanistan, not for Tajikistan's
  Tajik. NOT Latin: the 1928–1940 Latin era is long over; periodic government talk of a Latin or Perso-Arabic return has
  produced no production system and no timeline.
- Recommendation: Cyrillic only; don't build alternate scripts. Confidence: high.

Tajik is Persian in Cyrillic, terminology can lean Persian, script stays Cyrillic:
- Tajik shares most basic vocabulary with Persian and Dari; it's a variety of Eastern Persian written in a modified
  Cyrillic alphabet. So Persian/Dari term sense is a useful sanity check, but the script and the actual spelling are
  always Cyrillic. Never carry a Perso-Arabic spelling across; transliterate the sense into Cyrillic Tajik orthography.
- Recommendation: triangulate term meaning against Persian where the Tajik sources are thin, but render only in Tajik
  Cyrillic. Confidence: high.

Russian loanwords vs native Tajik/Persian terms:
- Like other ex-Soviet Central Asian languages, Tajik tech, government, and science vocabulary borrows heavily from
  Russian (an estimated 2,500 Russian loanwords, concentrated in technology, government, military, and medicine). So for
  any computing term there's often a Russian-loan option and a native Persian-rooted option.
- How the sources handle it for file-manager terms: the reference catalogs prefer native Persian-rooted terms here, not
  Russian loans. `файл` (file) is the one shared international loan; but `ҷузвдон` (folder), `ҷустуҷӯ` (search),
  `нусха бардоштан` (copy), `нест кардан` (delete), `интиқол додан` (move), `буридан` (cut) are all native
  Persian-rooted, agreeing across Microsoft and GNOME.
- Recommendation: prefer the native Persian-rooted term when the sources use one (they mostly do for file-manager
  vocabulary); accept established international/Russian loans only where they're the genuine norm (`файл`). Confidence:
  high for the file-manager core; native review needed for anything outside it.

Gender, none (simplifies translation):
- Tajik (Persian-family) has NO grammatical gender: no gendered nouns, adjectives, or pronouns, and a single
  third-person pronoun. So there's no gender agreement to thread through `{name}`/`{path}` placeholders, and no
  masculine/feminine variant problem. This removes a whole class of placeholder-agreement risk that gendered Slavic or
  Romance languages carry.
- Recommendation: no gender handling needed. Confidence: high.

Major-product localization is essentially absent (the priority signal):
- Apple does NOT ship Tajik: it's not a macOS UI display language and there's no Finder localization, so there's no
  Tier-1 (macOS) source at all. Microsoft has a terminology glossary (`tg-Cyrl-TJ`) but Tajik isn't a standard Windows
  display language. Google/Spotify/Netflix: no Tajik UI found. The only full-UI prior art is the GNOME Nautilus catalog
  (Tier 3), which is well-translated (~93% of strings).
- Recommendation: Cmdr would be a near-first-mover for a polished file-manager UI in Tajik. The reference base is
  thinner than for European languages (no Apple, partial Microsoft, one GNOME catalog). Flag for David: is shipping
  Tajik worth it given near-zero major-vendor precedent? Confidence: high that precedent is sparse; the go/no-go is
  David's.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Microsoft (`tg-Cyrl-TJ` terminology) and GNOME Nautilus agree on the
file-manager core, so confidence is high there; native review still gates shipping.

- file → файл · MS, GNOME · high
- folder → ҷузвдон · MS, GNOME · high
- copy → нусха бардоштан · MS, GNOME · high
- cut → буридан · MS · high
- paste → гузоштан · MS, GNOME · high
- delete → нест кардан · MS, GNOME · high
- move → интиқол додан · MS · high
- rename → тағйири ном · (Persian-rooted; native review) · tentative, sources thin for this exact term
- open → кушодан · MS, GNOME · high
- search → ҷустуҷӯ · MS, GNOME · high
- cancel (dialog button) → бекор кардан · GNOME · high, do NOT use Microsoft's `лағви интихоб`, which is the *deselect*
  sense, not a dialog Cancel
- trash → сабад · GNOME · high (literally "basket", matches the file-manager domain)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other` (verified: `new Intl.PluralRules('tg').resolvedOptions().pluralCategories` →
`['one', 'other']`, on Node 22, 2026-06-20). GNOME's catalog declares `nplurals=2; plural=(n != 1)`, which matches.
Author both `one` and `other` branches for every plural message. Tajik, like Persian, typically keeps the counted noun
singular after a numeral, so the two forms are often the same word, but the framework still needs both buckets. The
`desktop-i18n-plural` check requires every plural message to cover the categories this language needs.

## Notes and decisions

- Cyrillic with Tajik-specific letters: `Ғ ғ`, `Ӣ ӣ`, `Қ қ`, `Ӯ ӯ`, `Ҳ ҳ`, `Ҷ ҷ`. Don't substitute Russian look-alikes
  (for example `х`/`ҳ`, `г`/`ғ`, `к`/`қ`, `ч`/`ҷ`, `у`/`ӯ`, `и`/`ӣ` are distinct letters).
- No grammatical gender, so no gender agreement to manage around placeholders.
- Numbers and dates come from the formatter layer (Tajikistan conventions are Russia-influenced); never hardcode
  separators or date order.
- Record case-by-case rulings here as they're made.

## Decisions to confirm with David

- Go/no-go on shipping Tajik at all (no Apple, partial Microsoft, one GNOME catalog; near-zero major-vendor precedent).
- Ratify Cyrillic as the only target script (no Latin or Perso-Arabic build).
- `rename` term (`тағйири ном` is the Persian-rooted candidate; needs native confirmation).
- Native review of the full glossary and every placeholder-bearing string before ship.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in [`../guides/i18n-translation.md`](../guides/i18n-translation.md)
and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
