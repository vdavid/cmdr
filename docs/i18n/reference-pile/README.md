# i18n terminology evidence pile

Authoritative reference data for choosing translation terms with confidence, feeding the per-language translation style
guides at [`/docs/i18n/`](..) (`<tag>/style.md`). Goal: every term we pick can cite what a real localized OS or an
official vendor glossary actually says, so choices match user expectations instead of an agent's guess. Full inventory,
provenance, layout rules, and open items: `inventory.md`. Process and confidence model:
[`/docs/guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Must-knows

- **Language-first: one folder per locale gathers every source.** To research a language, point a translator at
  `_ignored/i18n/<tag>/` (e.g. `i18n/fr/`); inside are up to eight sources: `macOS/`, `microsoft-terminology/`,
  `microsoft-style-guides/`, `gnome-nautilus/`, `xfce-thunar/`, `total-commander/`, `double-commander/`, `kde-dolphin/`,
  present only where that source has the language. The first five cover most languages; the three file managers vary
  (`kde-dolphin/` 92 languages, `total-commander/` 48, `double-commander/` 30 — so e.g. `sv` has TC + Dolphin but no DC,
  `hu` and `fr` have all three). Check with `ls <tag>/`.
- **Mining recipes per source** (tested greps, jq, `msggrep`, `pdftotext`): `how-to-mine.md`. Use them; don't reinvent
  the search per term.
- **Reference for picking terms, never strings to copy.** Apple's and Microsoft's strings are copyrighted; the
  GNOME/Xfce catalogs are GPL. We read them to decide what term matches user expectations, then write Cmdr's own catalog
  value. Don't paste any vendor or upstream string verbatim into `apps/desktop/src/lib/intl/messages/`.
- **Locale key = BCP-47, lossless, base-preferred.** Tags are normalized from each source's native code (punctuation +
  script modifiers only: `pt_BR`→`pt-BR`, `sr@latin`→`sr-Latn`, legacy `no`→`nb`), with NO region↔script remapping. So
  multi-script/region languages stay as separate sibling folders (`zh-Hans` vs `zh-CN`, `sr-Cyrl` vs `sr-Latn`, `nb` vs
  `nb-NO`); those carry a `_see-also.txt` pointing to their siblings. This matches Cmdr's own `docs/i18n/<tag>/style.md`
  tag convention.
- **Gitignored, lives in the main clone.** `_ignored/` is untracked (`.gitignore` line 9), so this ~3 GB pile stays
  local, isn't subject to the doc-system checks, and belongs in the main clone, not a worktree (worktrees get cleaned).
- **Authority tiers** (how much a source proves "user expectation"): 1 = the real installed OS (macOS; strongest), 2 =
  vendor terminology + style guides (Microsoft), 3 = the file-manager-domain corpora (the explorer family GNOME
  Nautilus + Xfce Thunar + KDE Dolphin, and the orthodox two-pane pair Total Commander + Double Commander;
  cross-language parity, and the only sources that name two-pane concepts Finder lacks), 4 = native human review (the
  only thing that makes a term "confirmed"; out of budget for now). Within Tier 3, split by UI family: lean on the
  orthodox pair (TC, DC) for two-pane-specific terms (pane, file list, command line) where the OS vendors are silent,
  and on the explorer trio for general file operations.

## Structure

```
<tag>/<source>/…             one folder per BCP-47 locale (207 of them); see inventory.md for the sources
_extract/macos-extract/      reproducible macOS extractor — emits the <tag>/macOS/… layout (go run main.go)
_extract/reorg/              one-shot source-first → language-first restructure (already run; kept for reference)
_extract/competitor-fetch/   re-runnable fetch of KDE Dolphin + Total Commander + Double Commander (fetch.sh)
_downloads/                  raw MS Terminology zip + TC installer, kept for re-extraction
```
