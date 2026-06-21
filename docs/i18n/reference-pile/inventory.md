# i18n terminology evidence pile — details

Full inventory, provenance, layout rules, and open items for the reference pile. Must-knows and the top-level structure
map: [README.md](README.md).

## Layout and locale keys

The pile is language-first: `_ignored/i18n/<tag>/<source>/…`, 207 locale folders. Inside each `<tag>` the sources are
subdirs (`macOS/`, `microsoft-terminology/`, `microsoft-style-guides/`, `gnome-nautilus/`, `xfce-thunar/`,
`total-commander/`, `double-commander/`, `kde-dolphin/`), present only where that source has the language. A few folders
exist for a single source only: `ksh`/Kölsch (Total Commander), and `se`, `csb`, `hne`, `hsb`, `sr-Latn-ije` (KDE
Dolphin).

`<tag>` is a BCP-47 locale, derived losslessly from each source's native code — punctuation and script-modifier
normalization only, never region↔script remapping:

- **macOS / GNOME / Xfce**: `_`→`-` (`pt_BR`→`pt-BR`, `en_GB`→`en-GB`); `@mod`→`-Subtag` (`sr@latin`→`sr-Latn`,
  `ca@valencia`→`ca-valencia`, `uz@cyrillic`→`uz-Cyrl`); legacy macOS `no`→`nb`.
- **microsoft-terminology**: the TBX's authoritative internal `xml:lang` (already BCP-47: `zh-Hans`, `pt-BR`,
  `sr-Latn`).
- **microsoft-style-guides**: the slug→tag table in `_extract/reorg/main.go` — derived from terminology's codes by
  name-matching, plus an override map for Microsoft's regional/script splits its terminology lumps (`french-canada`→
  `fr-CA`, `spanish-mexico`→`es-MX`, `english-uk`→`en-GB`, `azerbaijani`→`az-Latn`, etc.). Unspecified-script slugs map
  to base (`punjabi`→`pa`, `uzbek`→`uz`, `sanskrit`→`sa`).
- **double-commander**: gettext `.po` filename codes, same normalization as GNOME/Xfce (`pt_BR`→`pt-BR`, `sr@latin`→
  `sr-Latn`, `zh_CN`→`zh-CN`, `zh_TW`→`zh-TW`).
- **kde-dolphin**: KDE's `po/<lang>/` dir codes, same `_`→`-` and `@mod`→`-Subtag` normalization (`en_GB`→`en-GB`,
  `ca@valencia`→`ca-valencia`, `uz@cyrillic`→`uz-Cyrl`, `be@latin`→`be-Latn`). Serbian ijekavian aligns to existing
  siblings: `sr@ijekavian`→`sr-ije`, `sr@ijekavianlatin`→`sr-Latn-ije`.
- **total-commander**: TC's own 3-letter codes (installer-bundled: `HUN`→`hu`, `DEU`→`de`, `CHN`→`zh-CN`, `SK`→`sk`
  Slovak, `SVN`→`sl` Slovene, `NOR`→`nb`, …) and the additional-language zip slugs (`wcmd_ptg`→`pt-BR`, `wcmd_tw`→
  `zh-TW`, `wcmd_loc_srb`→`sr-Cyrl`, `wcmd_loc_srl`→`sr-Latn`, `wcmd_esa`→`es-419`, `wcmd_koe`→`ksh`, …), name-matched
  to BCP-47. Watch the two false friends: `SK` is Slovak, `SVN` is Slovene (not Slovak/Slovenian-swapped).

### Lossless sibling families

Because we don't force region↔script merges, a few languages whose sources slice them differently end up as separate
sibling folders, each with a `_see-also.txt` listing the set:

- **Chinese**: `zh-Hans`/`zh-Hant` (Microsoft, script) vs `zh-CN`/`zh-TW`/`zh-HK` (macOS/GNOME, region).
- **Serbian**: `sr-Cyrl`/`sr-Latn`/`sr-Cyrl-BA` (Microsoft) vs `sr`/`sr-Latn`/`sr-ije` (GNOME).
- **Norwegian**: `nb`/`nn` (macOS/GNOME) vs `nb-NO`/`nn-NO` (Microsoft).
- **Portuguese**: `pt-BR` (Brazilian, complete) vs bare `pt` (European file-manager catalogs, `Language: pt`, plus an
  AppKit-only macOS) vs `pt-PT` (European macOS + Microsoft, no file managers). The bare `pt` is NOT a superset.
- **Bengali**: `bn` (Microsoft terminology + GNOME/Xfce/Dolphin) vs `bn-IN` (the Microsoft style guide). No macOS tier
  exists (Apple doesn't localize Bengali).

For de, sv, hu, and every plain base language there are no such splits: one clean folder each.

### Composed base folders for split catalog tags

Where a Cmdr catalog tag's evidence is split across the siblings above, the base folder is made mineable so the
how-to-mine recipes run verbatim against `<tag>/`. These are local conveniences (symlinks + a `_see-also.txt`); recreate
them after a pile rebuild:

- **`zh/`** (ships Simplified): symlinks `macOS` + the file managers → `zh-CN/`, and `microsoft-terminology` +
  `microsoft-style-guides` → `zh-Hans/`.
- **`bn/`**: symlinks `microsoft-style-guides` → `bn-IN/` (the rest is already in `bn/`).
- **`pt`**: no composed base — Cmdr's `pt` ships Brazilian, so mine the already-complete `pt-BR/` directly (the bare `pt`
  is European; don't mix).

### Scripts

- `_extract/macos-extract/` (`go run main.go`): harvests this Mac's bundles into `<tag>/macOS/<source>/…`. Re-runnable —
  clears every `<tag>/macOS` subtree and rewrites it, leaving other sources untouched.
- `_extract/reorg/` (`go run main.go`): the one-shot source-first→language-first restructure. Already run (it consumes
  the flat source dirs). Kept as the documented, reproducible mapping.

## What's collected

### macOS (Tier 1)

- **What**: localized UI strings from this Mac's system bundles, per language as JSON.
- **Layout**: `<tag>/macOS/<source>/<file>.json`. Sources harvested: `Finder`, `CoreTypes` (kind names like folder,
  volume), `AppKit` (standard buttons/menus: Cancel, Open, Eject, Move to Trash), `SystemSettings`.
- **Coverage**: 42 languages, 6,174 JSON files, ~32 MB. Includes sv, de, hu plus the full macOS language set.
- **Scope caveat**: curated to file-manager + standard-UI bundles, NOT every `.loctable` on the OS. Broaden by adding
  entries to the `sources` list in `_extract/macos-extract/main.go` and re-running.
- **Provenance**: extracted from `/System/…` on this machine via `plutil -convert json`, 2026-06-19. Re-run any time to
  refresh against the current macOS build.

### microsoft-terminology (Tier 2)

- **What**: Microsoft Terminology Collection, the full per-language TBX glossaries.
- **Layout**: `<tag>/microsoft-terminology/<LANGUAGE>.tbx` (e.g. `fr/microsoft-terminology/FRENCH.tbx`).
- **Coverage**: 111 languages, ~2.6 GB. Pretty-printed XML (`xmllint --format --huge`, 111/111) so it's browsable.
- **Provenance**:
  `https://download.microsoft.com/download/b/2/d/b2db7a7c-8d33-47f3-b2c1-ee5e6445cf45/MicrosoftTermCollection.zip`,
  downloaded 2026-06-19; upstream files dated 2024-11-06. The source zip is kept in `_downloads/` for re-extraction
  (note: re-extraction yields the original single-line TBX; re-run the `xmllint --format` pass after).
- **License**: Microsoft Terminology license (reference use; see the usage rule in README.md).

### microsoft-style-guides (Tier 2)

- **What**: Microsoft Localization Style Guides (tone, formality, conventions, do/don't) per language.
- **Layout**: `<tag>/microsoft-style-guides/StyleGuide.pdf`.
- **Coverage**: all 102 available languages, ~82 MB, 0 download failures. German (82 pp), Swedish (58 pp), Hungarian (62
  pp) among them.
- **Provenance**: `https://aka.ms/<language>-styleguide` redirects, downloaded 2026-06-19. Language list from
  https://learn.microsoft.com/en-us/globalization/reference/microsoft-style-guides.

### gnome-nautilus, xfce-thunar (Tier 3)

- **What**: translation catalogs (`.po`) for the two GTK file managers — exactly the file-manager domain, across many
  languages — the cross-language parity source (equal depth for languages David speaks and ones he doesn't).
- **Layout**: `<tag>/gnome-nautilus/nautilus.po`, `<tag>/xfce-thunar/thunar.po`.
- **Coverage**: Nautilus 123 languages (~28 MB), Thunar 67 languages (~10 MB).
- **License**: GPL (reference use; don't copy strings verbatim, same rule as the vendor sources).
- **Provenance**: shallow `git clone` on 2026-06-19, `po/*.po` copied out, clones then removed:
  - Nautilus `https://gitlab.gnome.org/GNOME/nautilus.git` @ `c4658b913a21740b874a4c955f51ff4494b8417b` (2026-06-19).
  - Thunar `https://gitlab.xfce.org/xfce/thunar.git` @ `7410dc9b93a6c56b39ad2d0c6e29ccfbe1a76862` (2026-06-18).
  - Re-clone to refresh.

### double-commander (Tier 3, file-manager domain)

- **What**: translation catalogs (`.po`) for Double Commander, the open-source orthodox two-pane manager — Cmdr's
  closest design lineage. Gettext, so it mines exactly like GNOME/Xfce.
- **Layout**: `<tag>/double-commander/doublecmd.po`.
- **Coverage**: 30 languages, ~12 MB.
- **License**: GPL (reference use; don't copy strings verbatim, same rule as the other sources).
- **Provenance**: `language/doublecmd.*.po` from `https://github.com/doublecmd/doublecmd` (`master`, raw fetch
  2026-06-20). Re-fetch the `language/` dir to refresh.

### kde-dolphin (Tier 3, file-manager domain)

- **What**: translation catalogs (`.po`) for KDE Dolphin. Single-pane explorer family (like Nautilus/Thunar), not
  orthodox two-pane — so weight it with the GTK pair for general file ops, not with TC/DC for two-pane terms.
- **Layout**: `<tag>/kde-dolphin/dolphin.po`.
- **Coverage**: 92 languages, ~22 MB — the broadest file-manager cross-language source in the pile.
- **License**: GPL (reference use; don't copy strings verbatim).
- **Provenance**: `po/<lang>/dolphin.po` from the GitHub mirror `https://github.com/KDE/dolphin` (`master`, raw fetch
  2026-06-21). The mirror carries a populated `po/` dir (KDE's scripty l10n sync), so no SVN or per-language l10n clone
  is needed — re-fetch `po/` to refresh.

### total-commander (Tier 3, file-manager domain)

- **What**: language files (`.lng`) for Total Commander, the archetypal orthodox two-pane manager. The richest source
  for two-pane concepts the OS file managers don't name (pane → "panel", file window, directory hotlist, button bar).
- **Layout**: `<tag>/total-commander/WCMD.LNG.utf8` (the strings) and `WCMD.INC.utf8` (the menu file), both decoded to
  UTF-8 from TC's native codepage.
- **Format**: INI-style `ID="value"` lines (numeric string IDs), prefixed by a header whose line 2 self-declares the
  codepage (`codepage=1250`). The IDs are not self-describing; grep the translated VALUES, or cross-reference an ID
  against `TOTALCMD.INC` (the English menu reference). See [how-to-mine.md](how-to-mine.md).
- **Coverage**: 48 languages, ~6 MB. 18 are bundled in the installer (incl. `de`, `hu`, `sv`, `fr`, `it`, `es`, `ru`,
  `ja`, …); the rest come from the per-language zips on the additional-languages page.
- **License**: proprietary (Ghisler). Reference use only; never paste a TC string into Cmdr's catalog — same rule as
  the vendor sources.
- **Provenance**:
  - Bundled set: `INSTALL.CAB` inside the 11.57 combined installer `https://totalcommander.ch/1157/tcmd1157x32_64.exe`
    (downloaded 2026-06-20), extracted with `7z`, each `LANGUAGE/WCMD_*.LNG` + `.INC` decoded by its declared codepage.
  - Additional set: per-language zips under `https://plugins.ghisler.com/languages/` (listed at
    `https://www.ghisler.com/languages.htm`), downloaded 2026-06-20.
  - Re-download the installer / zips to refresh against a newer TC version.

## Decisions made / open items

- **Orthodox file managers added as a domain source (2026-06-20).** Total Commander + Double Commander joined the pile
  because Cmdr is a two-pane orthodox manager and the OS file managers (Finder is single-pane) are silent on its core
  concepts — pane, file list, command line, directory hotlist. They're community/proprietary-translated, so they sit at
  Tier 3 (below the first-party OS vendors for general terms) but are the closest lineage match for orthodox-specific
  terms. Confirmed in practice: both render Hungarian "pane" as `panel`, which the Tier-1/2 sources never settled.
- **Lossless siblings, separate regional/script folders (2026-06-19).** Chosen over collapsing to base or force-merging
  scripts; the cost is that CJK/Serbian/Norwegian reference is spread across siblings (`_see-also.txt` bridges them),
  and the gain is zero data loss and no opinionated remap. Irrelevant to de/sv/hu.
- **Windows (Tier 1) — skipped, by decision (2026-06-19).** Reading the UTM VM's filesystem from here isn't feasible,
  and Microsoft Terminology + Style Guides (Tier 2) already capture Windows terminology authoritatively. Revisit only if
  a specific term needs the live Windows wording; then share a folder out of the VM (or mount its disk image) and
  harvest the `.mui` resource strings.
- **KDE Dolphin (Tier 3) — collected 2026-06-21.** The earlier "more work, skip it" call was wrong: the GitHub mirror
  `KDE/dolphin` carries a populated `po/<lang>/dolphin.po` (scripty l10n sync), so it fetches as cleanly as Nautilus,
  Thunar, and Double Commander — no SVN, no per-language l10n clone. At 92 languages it's now the broadest file-manager
  cross-language source. Single-pane explorer family, so it complements Nautilus/Thunar, not the orthodox TC/DC pair.

## Notes

- (scratch space for term-by-term findings, conflicts, and rulings as the glossary work proceeds)
