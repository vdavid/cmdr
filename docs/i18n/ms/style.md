# Malay (ms) translation style guide

Working notes for translating Cmdr into Malay. Read [`README.md`](../README.md) for how this fits the translation
process, and [`_template/style.md`](../_template/style.md) for the section contract.

The base tag is `ms`. Translate to standard Malaysian Malay (Bahasa Melayu Malaysia), which is the dominant software
locale; see the variant decision below before assuming Brunei or Indonesian conventions.

## Voice and tone

Friendly, concise, calm, active. Match Cmdr's English register: helpful and direct, never alarmist. Error messages stay
calm and actionable and avoid alarm words. Malay's natural equivalent of "error" / "failed" wording (`ralat`, `gagal`)
is heavy and blame-shaped, so prefer describing what happened and the next step, the same way the English copy avoids
those two words. Standard written Malay reads slightly more formal than casual English, but keep sentences short and
plain; avoid the dense bureaucratic register of government Malay.

## Formality

Malay has no T/V verb conjugation, but the second-person pronoun carries the register: `anda` is the neutral-formal
"you", `awak` / `kamu` are informal. UI convention, and what Apple (macOS) and the reference pile do, is twofold:

- **Default to pronoun-free imperatives** for actions and buttons. Malay imperatives are bare verb stems, so this is
  natural: Cancel = `Batal`, Copy = `Salin`, Choose = `Pilih`, Delete = `Padam`. Don't invent a pronoun where English
  has none.
- **Use `anda` when a second person is unavoidable** (settings descriptions, possessives like "your files"). This is
  Apple's choice throughout Finder ("Simpan folder Desktop & Dokumen `anda`"). Never use `awak` / `kamu` (too casual)
  and never the English "you".

Imperatives stay bare verb form (`Salin`, `Alih`, `Padam`), not the `-kan` causative or polite `sila` ("please") prefix,
which is for instructional prose, not buttons.

## Terminology and glossary

| English term | Malay          | Notes                                                                                          |
| ------------ | -------------- | ---------------------------------------------------------------------------------------------- |
| file         | fail           | Borrowed, standard everywhere (Apple, Microsoft, GNOME). NOT Indonesian `berkas`.              |
| folder       | folder         | Kept as-is across all majors. Don't nativize.                                                  |
| copy         | salin          |                                                                                                |
| move         | alih / pindah  | Apple uses both: `alih` (move within), `pindah` (relocate). Prefer `alih` for the in-app move. |
| delete       | padam          |                                                                                                |
| trash (noun) | sampah         | Apple "Sampah"; GNOME "Tong Sampah". Use `Sampah` to match macOS.                              |
| rename       | namakan semula | Apple form. (GNOME's `tukar nama` also exists; prefer Apple's.)                                |
| cancel       | batal          |                                                                                                |
| tab          | tab            |                                                                                                |
| pane         | anak tetingkap | Lit. "child of window"; the standard Malay UI term for a pane.                                 |
| volume       | volum          | Storage volume.                                                                                |
| settings     | tetapan        |                                                                                                |
| search       | cari           |                                                                                                |
| open         | buka           |                                                                                                |
| paste        | tampal         |                                                                                                |
| cut          | potong         |                                                                                                |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; the curated list is in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural category for `ms` is `other` only (verified with `new Intl.PluralRules('ms').resolvedOptions()`, Node,
2026-06). Malay has no plural inflection: number is unmarked, and reduplication (`fail-fail`) is for emphasis or
generality, not counting, so don't use it after a numeral. Every plural message needs only the `other` branch. The noun
stays singular in form after any count: "3 fail", "1 fail". The `desktop-i18n-plural` check requires the `other`
category.

Malay also has no grammatical gender, so an inserted name, path, or filename never forces gender agreement on the
surrounding sentence. That removes a whole class of translation hazard the German/Romance guides have to handle.

## Decision points

**1. Variant: use `ms` (Malaysian Malay) as the base; do NOT branch to ms-BN or conflate with Indonesian.** Confidence:
high. Malaysian Malay (Bahasa Melayu Malaysia) is the dominant software locale: Apple ships macOS in it (the reference
pile), Microsoft, Google, and Netflix all localize to it. Brunei Malay (ms-BN) is mutually intelligible and differs only
in minor vocabulary and spelling nuance, with negligible UI-string impact, so there's no reason to ship a separate ms-BN
catalog; the `ms` base covers Brunei users. Recommend a `ms-BN` variant only if a Brunei-specific need ever surfaces.

- **Indonesian (`id`) is a SEPARATE language, not a Malay variant.** Related and partly intelligible, but distinct in
  exactly the everyday file-manager words: file = `fail` in Malay but `berkas`/`file` in Indonesian; "search" = `cari`
  (shared) but many UI verbs and spellings diverge. Never seed `ms` from an Indonesian catalog or vice versa, and never
  treat `id` as an `ms` fallback. They get independent style guides and catalogs.

**2. Pronoun strategy: pronoun-free imperatives, `anda` only when a second person is unavoidable.** Confidence: high.
Matches Apple's macOS choice and the whole reference pile. See Formality above. David-only call: none; this is the
settled convention.

**3. Anglicism handling: follow the majors' established borrowings; don't nativize what the platform already borrowed.**
Confidence: high. Malaysian Malay borrows heavily from English for computing terms, and Apple/Microsoft/GNOME have
converged on a stable set: `fail` (file), `folder`, `tab`, `volum`, `format`. Keep those. Use the native term where the
majors do: `Salin` (copy), `Alih`/`Pindah` (move), `Padam` (delete), `Sampah` (trash), `Cari` (search), `Tampal`
(paste). Dewan Bahasa dan Pustaka (DBP) coins native alternatives (e.g. `pengkomputeran` for "computing"), but software
localization stays pragmatic and uses the borrowed term where it's what users actually read on screen. Don't reach past
the established UI term for a purer DBP coinage. David-only call: none, unless a specific term has no clear precedent in
the reference pile, in which case flag it for review rather than guessing.

**4. Localization depth is RICH; reference material is abundant.** Confidence: high. Apple (macOS, in the pile),
Microsoft (Windows + terminology TBX + style-guide PDFs for both ms-MY and ms-BN), Google, GNOME (Nautilus `.po`), XFCE
(Thunar `.po`), and Netflix all ship Malay. So almost every Cmdr term has a real, checkable precedent. Decide by
cross-referencing the pile (Apple first, since Cmdr is macOS-native), never by inventing a term.

## Notes and decisions

- Spelling: standard Malaysian Malay orthography (Bahasa Melayu Malaysia). No diacritics; Malay uses plain Latin
  letters, so no special-character handling beyond the borrowed source words.
- Punctuation, numbers, and dates follow the global style guide (ISO dates, Oxford-comma equivalent where a list needs
  it, thousands separators on user-facing counts). Malay doesn't use a different decimal/grouping convention that would
  override the project default here.
- When Apple and GNOME disagree on a term, prefer Apple's, since Cmdr is macOS-native and users carry Finder's
  vocabulary (e.g. trash = `Sampah` per Apple, not GNOME's `Tong Sampah`; rename = `namakan semula` per Apple).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ms/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
