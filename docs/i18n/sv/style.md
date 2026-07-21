# Swedish (sv) translation style guide

Working notes for translating Cmdr into Swedish. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Swedish.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them. Everything previously listed
here was resolvable from the reference pile and has moved into the glossary or notes below with its evidence; nothing
genuinely subjective remains open. Two near-calls worth a glance, both with a confident default:

- **`du` address, resolved (high).** Microsoft's Swedish style guide says outright "Rewrite to use the second person
  (du)" and sets a warm, informal tone; macOS Finder addresses the user with `du` throughout ("Du kan inte ångra den här
  åtgärden"). Both authorities agree, so this is settled, not pending. Stated here only so it's never relitigated.
- **`viewer` = `förhandsvisning` (high), not `granskare`.** See the glossary entry; flagging it here because it's the
  one term where macOS itself uses two words for nearby senses. If you ever want the file viewer to feel like a distinct
  "inspector" surface rather than a preview, `granskare` is the macOS-backed alternative.

## Voice and tone

Friendly, concise, active, calm. Swedish OS software is already informal and direct, so Cmdr's English voice carries
over cleanly. Keep sentences short and natural-spoken, not bureaucratic. Error messages stay calm and actionable and
never say "fel" (error) or "misslyckades" (failed) as a label the way English avoids "error"/"failed": phrase the
problem and the next step ("Det gick inte att byta namn på filen. Försök igen?"), not a status code.

## Formality

- **`du`, lowercase, throughout.** No `ni`, no formal address.
- **Buttons and menu items: imperative verb.** "Spara", "Avbryt", "Radera", "Byt namn", "Kopiera", "Flytta". This is the
  macOS/Windows Swedish norm. ("Radera" for permanent delete, "Flytta till papperskorgen" for the safe move; see the
  glossary's delete entry.)
- Avoid the passive-`-s` where an active imperative reads better ("Ta bort filen", not "Filen tas bort").

## Decision points

The localization calls for Swedish, beyond formality above. Most are settled (Swedish has no script, gender-agreement,
or RTL complications, so the surface is small); the one genuinely open call is the regional variant. Evidence verified
against the reference pile (`_ignored/i18n/sv/`) on 2026-06-20.

- **Regional variant: target `sv-SE` (Sweden), don't split out `sv-FI` (high; the one call worth confirming).**
  - Options: a single Sweden-Swedish catalog under `sv`/`sv-SE`, vs a separate Finland-Swedish (`sv-FI`) variant.
  - Majors: Microsoft and Apple both ship Sweden-Swedish as the single `sv` and do not maintain a separate
    Finland-Swedish UI; `sv-FI` exists as a locale (Euro currency, UTC+2, some legal-term and vocabulary differences)
    but vendors localize the UI once for `sv` and let the number/date/currency formatter handle the regional split.
    Google, Spotify, and Netflix do the same: one Swedish UI, regional formatting from the locale.
  - Recommendation: ship one Swedish catalog targeting Sweden-Swedish. Finland-Swedish differences that matter to Cmdr
    (currency, date, thousands separators) already come from `formatNumber()`/`formatBytes()`, not from catalog strings,
    so a separate `sv-FI` catalog would duplicate near-identical text for no real gain. Tag the catalog `sv` (base) so
    it serves every Swedish region as the fallback.
  - Flag for David only if Cmdr ever wants a deliberately Finland-Swedish presence; otherwise this is settled.
- **Formality: `du`, informal, throughout (high, settled).** Both authorities agree: Microsoft's Swedish style guide
  says "Rewrite to use the second person (du)" and macOS Finder addresses the user as `du`. No `ni`. See Formality
  above; stated here so it's never relitigated.
- **Gender and inclusive language: a non-issue in Swedish UI (high).** Swedish UI strings don't gender the user the way
  Slavic or Romance languages do (no past-participle or adjective agreement with the user's gender in the phrasings Cmdr
  uses). The one live point is `en`/`ett` noun gender driving article and adjective agreement inside plural/count
  branches ("en markerad fil" vs "ett markerat objekt"); keep agreement correct per noun inside each ICU branch (see
  Plurals). No gender-neutral-rewrite strategy is needed beyond that.
- **Script, capitalization, length: no special handling (high).** Latin script, no RTL. Sentence case is native (Swedish
  doesn't capitalize common nouns, days, or months), so the app-wide sentence-case rule applies without friction. Length
  runs close to English, so overflow risk is lower than German but still overflow-check against the pseudolocale
  (`en-XA`).

## Terminology and glossary

Swedish IT terminology follows Svenska datatermgruppen and Apple/Microsoft Swedish. Prefer the macOS term when macOS and
Windows differ, since Cmdr is a macOS app; the Windows/GNOME variant is noted when it's a real split worth knowing.

Format per term: `chosen · sources · confidence`. Confidence is `confirmed` (a native human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). Evidence is verified against the
reference pile (`_ignored/i18n/sv/`) on 2026-06-19; macOS strings cited are what Swedish Finder/AppKit actually show.
Sources read but never copied verbatim (Apple/MS copyrighted, GNOME/Xfce GPL): they decide the term, Cmdr writes its own
value.

Settled terms (sources agree, all `high`):

- **pane: `panel`** · macOS uses "panel" for setting panes and AppKit/Thunar both use it for window regions (Thunar:
  "Sidopanel", "delad panel"); MS's primary "pane" sense is also "fönster"/panel. The two file lists. Keep "panel"
  consistently; don't mix in "ruta".
- **tab: `flik`** · macOS AppKit ("Ny flik", "Stäng flik", "Flikfält"), Thunar agree. `high`.
- **volume: `volym`** · macOS Finder ("Servervolymer"), MS terminology ("volym"). A mounted disk volume. `high`.
- **drive: `enhet`** · macOS ("anslut en enhet"), MS ("enhet"). Physical/removable drive. Note: macOS also says "skiva"
  for a disk and "hårddisk" for hard disk; reserve "enhet" for the device, "skiva" only when mirroring Finder's disk
  wording. `high`.
- **folder: `mapp`** · macOS Finder ("Ny mapp", "Mapp", "Flytta till papperskorgen"), MS, GNOME, Thunar all agree.
  Definite "mappen", plural "mappar". `high`.
- **directory: `katalog`** · MS terminology ("katalog"); use only where the technical filesystem sense matters, else
  "mapp". `high`.
- **file: `fil`** · everywhere. Definite "filen", plural "filer". `high`.
- **listing: `fillista`** · the file list inside a pane. No source has "listing" as a noun directly, but Thunar/Nautilus
  render "List View" as "Listvy" and Swedish IT routinely compounds "fil" + "lista". "fillista" reads natural for the
  list of files; use "listvy" only for the list-vs-icon view mode itself. `tentative` (compound by convention, no direct
  source term), but low risk.
- **trash: `papperskorgen`** · macOS Finder, GNOME, Thunar all use "papperskorgen" (definite, as Apple shows it). macOS
  also carries "Skräp"/"Borttagna objekt" in places, but "papperskorg" is the dominant Finder term. `high`.
- **move to trash: `flytta till papperskorgen`** · macOS Finder and GNOME both use this exact phrasing. `high`.
- **eject: `mata ut`** · macOS Finder ("Mata ut"), GNOME ("Mata ut") agree. `high`.
- **disconnect: `koppla från`** · macOS, MS ("koppla från"), GNOME ("Koppla från") all agree. Network server/share.
  `high`.
- **server: `server`** · macOS ("Anslut till server", "Serveradress", "Servernamn"). Connect-to-server verb is "Anslut".
  `high`.
- **bookmark: `bokmärke`** · MS ("bokmärka" verb), GNOME ("bokmärken"), Thunar ("Lägg till bokmärke") agree. Noun
  "bokmärke", plural "bokmärken". `high`.
- **sort: `sortera`** · macOS ("Sortera efter"), MS, Thunar ("Sortera objekt i stigande/fallande ordning"). `high`.
- **settings: `inställningar`** · macOS Finder ("Inställningar…"). Apple's current term. `high`.
- **cancel: `avbryt`** · macOS Finder ("Avbryt"), MS ("Avbryt"). Imperative on buttons. `high`.
- **overwrite: `skriv över`** · macOS Finder ("Skriv över", "Skriv över tillägg", "ska behållas eller skrivas över"), MS
  ("skriva över"). `high`.
- **index / indexing: `index` / `indexering`** · MS terminology ("index"); standard Swedish IT compound for indexing.
  `high`.
- **search: `sök` (verb) / `sökning` (noun)** · macOS Finder ("Sök", "Sök efter namn…", "Vid sökning"), MS ("sökning").
  `high`.
- **download: `hämta` (verb) / `hämtning` (noun)** · macOS Finder uses "Hämtade filer" for the Downloads folder and
  "Hämtningar" for the Recents/downloads list. Folder-name contexts may mirror "Hämtade filer"; the action is "hämta".
  `high`.
- **transfer: `överföring`** · the copy/move-in-progress noun. macOS Finder frames these as "Kopiera"/"Flytta"
  operations and shows progress without a single noun; "överföring" is the natural Swedish IT term for a transfer in
  flight. (MS terminology's "överlåtelse" is the legal/ownership sense, not this one, so it's the wrong fit.) `high`.

Near-calls (one real split, resolved with the macOS-wins rule):

- **delete: `radera`** (permanent delete) · macOS Finder/AppKit use "Radera" for permanent delete ("Radera direkt…" =
  Delete Immediately, "Töm papperskorgen" = Empty Trash, "Radera skivan" = Erase Disk). MS and GNOME/Thunar instead use
  "Ta bort" for delete. macOS wins here since Cmdr is a macOS app: use **`radera`** for the destructive permanent
  delete, and **`flytta till papperskorgen`** (above) for the safe move-to-trash. Reserve "ta bort" for removing an item
  from a list/collection (for example "ta bort från bokmärken", as GNOME does), not for deleting files from disk.
  `high`.
- **viewer: `förhandsvisning`** · macOS Finder uses "Förhandsvisning" for the preview pane ("Visa/Göm förhandsvisning")
  and "granskare" for the inspector ("Visa granskare"). Cmdr's file viewer is a preview surface, so
  **`förhandsvisning`** is the fit; "granskare" is the macOS-backed fallback if it ever becomes a distinct inspector.
  Quick Look stays "Quick Look" (brand). `high`.
- **share (network): `delad mapp`** · macOS Finder shows "Delad mapp" for a shared folder and "Delad"/"Delat" for the
  shared state; an SMB mount surfaces to the user as a shared folder. (MS terminology's "aktie" for "share" is the
  financial sense, irrelevant here.) Prefer **`delad mapp`** for the user-facing SMB share; "delad resurs" is acceptable
  where "resource" generality is wanted, but "delad mapp" reads more natural in Finder's voice. `high`.

Add terms as they come up, in this same `chosen · sources · confidence` shape; keep the whole catalog consistent with
the agreed choice.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`. The macOS UI names Cmdr opens into (System Settings panes, "Papperskorgen")
should match what a Swedish macOS actually shows.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('sv')`). Write both branches.

- Swedish plural form depends on the noun's declension and isn't a simple "+r": "1 fil" / "2 filer", but "1 objekt" / "2
  objekt" (neuter nouns ending in a consonant often don't change). Write the natural plural for each noun, don't
  pattern-match off English.
- `en`/`ett` gender affects agreement ("en markerad fil" vs "ett markerat objekt"). Keep article and adjective agreeing
  with the counted noun inside each branch.

## Notes and decisions

- **Sentence case is native.** Swedish doesn't capitalize common nouns, days, or months, so the app's sentence-case rule
  applies without friction. Don't title-case.
- **Quotation marks: `”…”`** (right double quote both sides) is the standard Swedish form. Avoid English `"…"`.
- **Percent sign: always a space before `%`** ("100 %", "{percent} %"). Swedish typography, and what the rest of the sv
  catalog does. Don't carry English's tight `50%` across, even inside a placeholder-heavy string.
- **Warning badges are noun-shaped, not imperative.** A compact badge beside a row names a STATE, so it takes a noun
  ("(överskrivning!)"), never the imperative that would double as a command to the user ("(skriv över!)"). The
  underlying action verb (`skriv över`) is unchanged on buttons and menu items.
- **Numbers and dates come from the formatter layer.** Swedish uses a comma decimal and space thousands separator (1
  000), but `formatNumber()`/`formatBytes()` produce these from the locale: never hardcode separators in a string.
- **Length.** Swedish runs close to English in width, so overflow risk is lower than German, but still overflow-check
  the layout against the pseudolocale (`en-XA`).
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/sv/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
