# Persian / Farsi (fa) translation style guide

Working notes for translating Cmdr into Persian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Persian.

**RTL, and no macOS reference.** Apple does NOT ship a Persian macOS UI, so the pile has no macOS Finder
(`_ignored/i18n/fa/` has GNOME Nautilus + MS terminology + MS style guide only; `fa-IR/` has Xfce Thunar; `prs-AF/` has
MS terminology for the Afghan Dari variant). The highest-authority source (a real localized OS) is absent, so terms lean
on GNOME + Microsoft and stay a notch less certain than for a macOS-backed language. RTL is a layout workstream, not
just translation. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

The calls a translator can't make alone. These are real open flags, not pre-settled defaults.

- **RTL readiness gates shipping Persian at all (high).** This is the headline. Persian needs full UI mirroring (see the
  RTL decision point); it's an app-code workstream separate from translation, shared with any future Arabic/Hebrew/
  Pashto/Urdu locale. Don't ship Persian until the app's RTL layout is verified end to end.
- **Numerals: Western (0-9) vs Persian (۰-۹) digits (tentative).** A file manager shows counts and sizes constantly.
  Persian users mix both in practice; auto-converting everything to Persian digits is wrong. Needs a native call on what
  the target audience expects (see decision point). Flag for David.
- **Address form: polite plural recommended (high), worth a sign-off.** Persian has a T-V distinction; software uses the
  polite register via plural verb endings (see Formality). Recommended default below, but it sets the tone for every
  sentence.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. The Persian Microsoft voice asks the
user "casually and politely" to take actions (MS style guide, verified 2026-06-20), warm but respectful, which fits
Cmdr. With no macOS reference, prioritize clear, plain Persian over clever phrasing. Error messages stay calm and
actionable: phrase the problem and the next step, and avoid "خطا" (error) / "ناموفق" (failed) as a bare status label the
way English avoids "error"/"failed".

## Formality

Persian distinguishes informal (تو, singular "you") from polite (شما, grammatically plural "you"), and **software uses
the polite شما register**, expressed through plural verb endings rather than a spelled-out pronoun.

- **The MS Persian examples use polite plural imperatives** addressed to the user: "میخواهید ادامه دهید؟" (Do you want
  to continue?), "آن را بررسی کنید و دوباره امتحان کنید" (check it and try again), the `-ید` plural ending is the polite
  form (MS style guide, verified 2026-06-20). Use this throughout. Never the informal تو / singular `-ی` ending.
- **Action labels (buttons, menu items): the polite imperative or a verbal noun.** GNOME Persian uses verbal nouns and
  imperatives: "تغییر نام" (rename, lit. name-change), "بیرون دادن" (eject), "ترتیب" (sort), "انتقال به زباله‌دان" (move
  to trash) (GNOME Nautilus, verified 2026-06-20). Both noun-style ("تغییر نام") and polite-imperative labels are
  idiomatic; prefer the concise verbal-noun form for single-word actions, matching GNOME.
- **Sentences to the user: polite plural, pronoun usually dropped.** Persian is pro-drop; the polite ending on the verb
  carries the address, so you rarely spell out شما. "آیا مطمئنید که می‌خواهید این پرونده‌ها را حذف کنید؟" (Are you sure
  you want to delete these files?).
- So the rule: **labels = verbal noun / polite imperative; sentences = polite plural, pronoun dropped; never the
  informal تو form.** Confidence: high.

## Decision points

### RTL: the dominant concern

Persian is written right-to-left in the Perso-Arabic script. This is the single biggest issue and it's a LAYOUT concern
as much as a text one (shared verbatim with [`ps/style.md`](../ps/style.md), sd-Arabic, and any future
Arabic/Hebrew/Urdu):

- The whole UI must mirror: panes swap sides, cursor/selection logic, progress bars, chevrons, and "back/forward"
  navigation arrows all reverse. A right-pointing "forward" arrow is wrong in RTL.
- Cmdr is a two-pane file manager, so the left/right pane mental model itself mirrors under RTL. Confirm the app's
  layout engine flips correctly before shipping any RTL locale; this is an app-code question, not a translation one.
- **Bidi hazard: file paths, URLs, brand names (Cmdr, macOS, SMB), and Western numbers are LTR runs embedded in RTL
  text.** Without proper Unicode bidi isolation, a `{path}` insert visually scrambles the surrounding sentence. A
  documented, concrete failure: two-digit numbers in RTL render reversed (16→61, 18→81, 20→02) when bidi handling is
  wrong (web sources, verified report 2026-06-20). Ensure every `{path}`, `{count}`, `{size}`, and brand token is
  bidi-isolated (Unicode FSI/PDI or equivalent).
- Recommendation: do NOT ship Persian until the app's RTL layout mirroring AND bidi isolation are verified end to end.
  The translation is the smaller half of the work. Confidence: high that RTL is the gating issue.

### ZWNJ (half-space / نیم‌فاصله): mandatory intra-word joiner

Persian uses the Zero-Width Non-Joiner (U+200C, "half-space" / نیم‌فاصله) to separate word parts that must read as one
word without a full space joining them (MS style guide explicitly requires it, verified 2026-06-20: "words composing of
several parts shall not be separated… using ZWNJ"). Examples: "می‌خواهید", "پرونده‌ها", "نرم‌افزار".

- Translators MUST type the real ZWNJ, not a regular space and not a hyphen. Plural suffixes (ها), prefixes (می, نمی),
  and compounds attach via ZWNJ.
- This is invisible in most editors; getting it wrong produces text that reads as broken/amateur to a native.
  Confidence: high (it's a hard requirement, not a preference).

### Numerals: Western vs Persian digits

Persian has its own digits (۰۱۲۳۴۵۶۷۸۹, U+06Fx), which are a DISTINCT Unicode block from the Eastern-Arabic digits
Arabic uses (web sources, verified 2026-06-20), don't reuse Arabic numerals for Persian.

- File sizes and counts appear constantly. Persian speakers commonly MIX Western and Persian digits, so auto-converting
  everything to Persian digits is not safe (web sources, verified 2026-06-20).
- Recommendation: let `Intl` with the `fa` locale decide numeral shaping (it produces Persian digits by default) rather
  than hardcoding either, and confirm with a native reviewer whether the target audience prefers Persian or Western
  digits for technical values like byte counts. Confidence: tentative (a genuine audience call).

### Script and regional variant

- **Script: Perso-Arabic only, no decision.** No Latin alternative in use. Persian adds four letters to Arabic (پ چ ژ گ)
  and shapes some shared letters differently (ك→ک, ي→ی), use the Persian forms, not the Arabic ones (a common encoding
  slip). Confidence: high.
- **Variant: target Iranian Persian `fa` (`fa-IR`).** Dari (Afghanistan, `prs`/`prs-AF`) and Tajik (Cyrillic, separate)
  are distinct enough that Microsoft maintains a separate `prs-AF` terminology set. Iranian Persian is the largest
  audience and the natural base. Don't fold Dari in; if Afghanistan demand appears, it's a separate variant. Confidence:
  high.

### Gender and inclusive language

Persian is **grammatically genderless**, no gendered pronouns (او = he/she/it), no gendered verb or adjective agreement.
This is a real advantage: none of the gender-guessing problems that plague Polish/German UI exist here. Nothing to
engineer around. Confidence: high.

### Capitalization

Perso-Arabic script has no letter case, so the whole sentence-case/title-case question is moot. Cmdr's sentence-case
rule has nothing to apply to here. Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Confidence: `confirmed` (native sign-off), `high` (authoritative
sources agree), `tentative` (sources conflict or none had it). **No macOS source exists**, so the ceiling without a
native reviewer is effectively `high` from GNOME+MS agreement, never macOS-backed. Evidence from `_ignored/i18n/fa/`
(GNOME Nautilus, MS terminology, MS style guide), verified 2026-06-20. Sources decide the term; Cmdr writes its own
value (MS copyrighted, GNOME GPL, never copied verbatim). All terms below should carry ZWNJ where shown.

Settled terms (GNOME + MS agree):

- **folder: `پوشه`** · MS terminology; GNOME uses "شاخه" (branch/directory) but "پوشه" is the dominant modern term for
  folder. Prefer **`پوشه`**. `high`.
- **file: `پرونده`** · GNOME ("پرونده"). MS also uses "فایل" (loanword). "پرونده" is the native term; "فایل" the
  loanword, both common. Prefer "پرونده" to match GNOME. `high`.
- **trash: `زباله‌دان`** · GNOME ("زباله‌دان", with ZWNJ). `high`.
- **move to trash: `انتقال به زباله‌دان`** · GNOME ("انتقال به زباله‌دان"). `high`.
- **delete: `حذف`** · MS/GNOME ("حذف"). `high`.
- **eject: `بیرون دادن`** · GNOME ("بیرون دادن"). `high`.
- **rename: `تغییر نام`** · GNOME ("تغییر نام"). `high`.
- **sort: `مرتب‌سازی` / `ترتیب`** · GNOME ("ترتیب"). Prefer "مرتب‌سازی" (with ZWNJ) for the action. `high`.
- **sidebar: `نوار جانبی`** · GNOME ("نوار جانبی"). `high`.
- **bookmark: `نشانک`** · GNOME ("نشانک"). `high`.
- **copy: `رونوشت` / `کپی`** · MS "رونوشت"; "کپی" (loanword) also common. `tentative` (no macOS to break the tie).
- **open: `باز کردن`** · MS/GNOME. `high`.
- **cancel: `لغو`** · MS ("لغو"). `high`.
- **save: `ذخیره`** · MS ("ذخیره"). `high`.
- **search: `جستجو`** · MS/GNOME ("جستجو"). `high`.

Tentative / needs a native check (thin evidence, no macOS):

- **volume: `حجم` / `درایو`** · "حجم" also means "size/volume(amount)", ambiguous for a mounted volume; "درایو" (drive,
  loanword) may read clearer. `tentative`.
- **tab (UI tab): `زبانه`** · standard Persian UI for tabs; the keyboard Tab is separate. `tentative`.
- **pane: `قاب` / `پنجره`** · the two file lists; no direct source term. `tentative`.
- **listing: `فهرست پرونده‌ها`** · reads natural for the file list; no canonical source term. `tentative`.
- **directory: `شاخه`** · GNOME's word for folder/directory; use only where the technical sense matters, else "پوشه".
  `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. In RTL text these LTR brand runs need bidi isolation (see RTL decision
point). There's no macOS Persian UI to match macOS-name strings against, so brand/system names stay as their Latin
forms.

## Plurals

CLDR categories for `fa`: `one`, `other` (verified with `new Intl.PluralRules('fa')`; GNOME's nplurals=2 agrees). Two
forms.

- **one**: 0 and 1 (`i=0..1`). Persian groups 0 with the singular. "۱ پرونده" / "1 file".
- **other**: 2 and up. "۵ پرونده".
- **Note: the counted noun usually does NOT add a plural suffix after a numeral** in Persian ("۵ پرونده", not "۵
  پرونده‌ها"), the number already marks plurality. Keep both branches reading naturally; don't reflexively append "ها"
  in the `other` branch. The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- **Quotation marks: `«…»`** (guillemets, U+00AB/U+00BB), the standard Persian form, and they keep their visual
  direction under RTL (the "opening" guillemet still faces into the quoted text). Avoid straight ASCII `"` and English
  `"…"`.
- **Numbers and dates come from the formatter layer.** `formatNumber()`/`formatBytes()` with the `fa` locale produce
  Persian digits and the locale's separators; never hardcode. (See the numerals decision point for the
  Persian-vs-Western audience call.) Calendar: Iran uses the Solar Hijri (Jalali) calendar, if Cmdr ever shows
  human-facing dates, that's a separate decision to flag, not just a format swap.
- **ZWNJ everywhere it belongs** (see decision point): plural/verb affixes and compounds. This is the most common silent
  quality bug in Persian UI.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/fa/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
