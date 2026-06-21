# How to mine the reference pile

Tested recipes for extracting a term or convention from each source in `<tag>/`. The point: don't read whole files or
reinvent the search each time. Triangulate a term across every source the language has, then record your choice with
sources and a confidence in the per-language style guide (`docs/i18n/<tag>/style.md`). Structure and provenance of the
pile: [README.md](README.md) and [inventory.md](inventory.md).

First check which sources your language has: `ls <tag>/` (a source is absent if its subdir is missing). Run everything
below from `_ignored/i18n/`.

## Source-quality traps (read before mining ANY language)

These are evidence-quality pitfalls found while preparing wave 1 (2026-06-21). Each has burned a translation pass; check
for them before trusting a source.

1. **The base tag can be incomplete, or the WRONG variant — always look for siblings.** Run `ls -d <tag>*` and read any
   `_see-also.txt`, not just `ls <tag>/`. A language's evidence is often split across region/script siblings:
   - **pt**: bare `pt/` is EUROPEAN (`Language: pt`, and an AppKit-only macOS with no Finder). The complete Brazilian
     set is `pt-BR/`. Cmdr's `pt` ships Brazilian, so mining bare `pt/` is a variant trap — mine `pt-BR/`.
   - **zh**: there is no bare `zh/` from the sources; Simplified is split across `zh-Hans/` (Microsoft) + `zh-CN/`
     (macOS, file managers), Traditional across `zh-Hant/` + `zh-TW/`. A composed `zh/` base (symlinks to the Simplified
     sources) exists for convenience — see [inventory.md](inventory.md) § Composed base folders.
   - **bn**: bare `bn/` has Microsoft terminology + the file managers; the Microsoft style guide is under `bn-IN/`.
2. **No macOS tier for every language.** Apple doesn't localize into some languages (e.g. Bengali: `bn/macOS/` doesn't
   exist). When `<tag>/macOS/` is absent, the Tier-1 "macOS wins" tiebreak is gone — Microsoft terminology becomes the
   anchor, the file managers carry the file-manager domain, and more terms honestly stay `tentative`. Don't read the
   absence as a pile gap; it's the source ceiling.
3. **macOS bundles contain English-valued files (Siri intents).** A non-`en` `<tag>/macOS/Finder/` mixes in
   English-valued Siri-intent phrase files (`AssistantIntents`, `#!SET#!_`-prefixed keys), so a naive value-grep on the
   target folder returns English and looks like a missing translation. Use the KEY-based cross-reference: find the key
   in `en/macOS/`, then read the SAME key in `<tag>/macOS/` (keys are stable across languages). The macOS recipe below
   does this.
4. **Microsoft terminology's first hit is often the wrong SENSE.** A `.tbx` term has multiple senses and the first match
   isn't always the UI one. Seen in fr: item → "article" (publishing sense) where the UI wants macOS "élément"; sidebar
   → "encadré" (callout) vs macOS "barre latérale"; share → "part" vs macOS "partage". Disambiguate against macOS Tier-1
   (Cmdr is a macOS app) and record which sense you took.
5. **Formality is per-language and OS-driven — never carry one language's call to another.** German macOS is informal
   `du` (Microsoft's formal `Sie` is the Windows convention, not Cmdr's); French macOS AND Microsoft are both `vous`.
   Mine the actual native-OS register for each language; don't generalize.
6. **A Cmdr catalog tag may not equal the pile folder to mine.** `pt` → mine `pt-BR/`; `zh` → mine the composed `zh/`
   (Simplified). When substituting the tag into the translator-agent prompt's "mine `_ignored/i18n/<tag>/`" line, use
   the variant-correct folder, which each language's `style.md` names.

For WHICH term to pick once you've mined the candidates (localize the Apple feature names Apple localizes; prefer the
macOS Finder term over the Windows/Microsoft one; let brand names inflect), see
[`/docs/guides/i18n-translation.md`](../../guides/i18n-translation.md) § Term-choice principles. Trap 5 above is the
formality instance of the macOS-over-Windows rule.

(Trap 1's "Double Commander directory hotlist is a different feature, not a bookmark" lives in the four mining gotchas
in [`/docs/guides/i18n-translation.md`](../../guides/i18n-translation.md) § Researching terms.)

## macOS (Tier 1, strongest) — `<tag>/macOS/<bundle>/*.json`

Flat `key: value` JSON per bundle (`Finder`, `AppKit`, `CoreTypes`, `SystemSettings`). Keys are stable across languages,
so cross-reference English→target by key. The `en/` folder is the English side.

```sh
# 1. Find the key for an English term (which bundle/file, and the key):
grep -rl '"Eject"' en/macOS/                    # which file has it
grep -i 'eject' en/macOS/AppKit/AccessibilityImageDescriptions.json   # -> "NSNavEjectButton": "eject"
# 2. Read the target translation of that same key:
grep '"NSNavEjectButton"' sv/macOS/AppKit/AccessibilityImageDescriptions.json   # -> "mata ut"
```

Or with jq, search value strings and print key+value:

```sh
jq -r 'to_entries[]|select((.value|type)=="string" and (.value|test("eject";"i")))|"\(.key)\t\(.value)"' \
  sv/macOS/Finder/*.json
```

macOS is the highest-authority source: it's what a user literally sees in Finder. Prefer it when sources disagree.

## Microsoft terminology (Tier 2) — `<tag>/microsoft-terminology/<LANG>.tbx`

Pretty-printed TBX XML, no namespace. Each `<termEntry>` has two `<langSet>`: `en-US` first, then the target. So in a
window after the English `<term>`, the next `<term>` is the translation. `termNote type="geographicalUsage"` flags
region (e.g. `AUT, DEU, CHE` for German), and `descrip type="definition"` gives the sense.

```sh
# English -> target (read the second <term> in the window, and any geographicalUsage):
grep -i -A14 '<term[^>]*>folder</term>' de/microsoft-terminology/GERMAN.tbx | grep -iE '<term|xml:lang|geographicalUsage'
#   -> folder ... <langSet xml:lang="de"> ... Ordner ... AUT, DEU, CHE, LUX

# Validate a candidate target term exists, and see its English source (-B = lines before):
grep -i -B14 '<term[^>]*>Ordner</term>' de/microsoft-terminology/GERMAN.tbx | grep -iE '<term'
```

Files are large; grep (streaming) beats loading them. `xmllint --xpath` works too but reads the whole doc into memory.

## Microsoft style guide (Tier 2) — `<tag>/microsoft-style-guides/StyleGuide.pdf`

Use for tone, formality (how to address the user), capitalization, and grammar conventions — not single terms. Extract
text once, then grep; or open sections with the Read tool (it renders PDF pages).

```sh
pdftotext de/microsoft-style-guides/StyleGuide.pdf - | grep -iE -A3 'addressing the user|formal|du-form|anrede|tilltal'
```

The high-value sections are the early style/tone/grammar chapters and the "addressing the user" / formality section.

## GNOME / Xfce (Tier 3, cross-language parity) — `<tag>/gnome-nautilus/nautilus.po`, `<tag>/xfce-thunar/thunar.po`

gettext catalogs (`msgid` English, `msgstr` translation). Exactly the file-manager domain. Use `msggrep` (cleaner than
grep for multi-line and plural entries):

```sh
msggrep --msgid -e 'Eject' sv/gnome-nautilus/nautilus.po          # entries whose msgid matches
msggrep --msgstr -e 'papperskorg' sv/gnome-nautilus/nautilus.po   # reverse: find by target word
grep -A2 'Plural-Forms' sv/gnome-nautilus/nautilus.po             # the language's plural rule
```

Plural entries use `msgid`/`msgid_plural` with `msgstr[0]`, `msgstr[1]`, … — good evidence for how a real catalog
phrases counted strings in your language.

## Double Commander + KDE Dolphin (Tier 3, gettext) — `<tag>/double-commander/doublecmd.po`, `<tag>/kde-dolphin/dolphin.po`

Same gettext format as GNOME/Xfce, so the same `msggrep` recipes apply — just point at the file. Pick by UI family:
**Double Commander** is orthodox two-pane (Cmdr's lineage — the place to look for terms Finder doesn't have); **KDE
Dolphin** is single-pane explorer family (broadest coverage at 92 languages — weight it with Nautilus/Thunar for general
file ops, not for two-pane terms).

```sh
msggrep --msgid -e 'panel' -i hu/double-commander/doublecmd.po | grep -E '^msg(id|str) '   # pane → "panel" (orthodox)
msggrep --msgid -e 'file list' -i hu/double-commander/doublecmd.po                          # file list → "fájllista"
msggrep --msgid -e 'Move to Trash' -i hu/kde-dolphin/dolphin.po                             # general op, broad coverage
```

## Total Commander (Tier 3, orthodox file manager) — `<tag>/total-commander/WCMD.LNG.utf8`

INI-style `ID="value"` lines (numeric string IDs), already decoded to UTF-8. The IDs aren't self-describing, so mine by
the translated VALUE rather than by key. `WCMD.INC.utf8` is the menu file (menu labels with `&` accelerators), often the
cleanest place to see a term in a real menu:

```sh
# Find how a concept is phrased by grepping the target value (TC is the richest source for two-pane terms):
grep -iE 'panel' hu/total-commander/WCMD.LNG.utf8        # pane → "panel" (e.g. "az aktív panelről")
grep -iE 'könyvjelz|kedvenc|favorit|hotlist' hu/total-commander/WCMD.INC.utf8   # bookmark/favorites framing
```

To pin an English source to a TC ID, cross-reference `TOTALCMD.INC` (the English menu reference in the installer CAB);
TC ships no English `WCMD.LNG` (English is compiled in), so there's no English-side string file to diff against — value
grep plus the menu file is the practical path.

## Confidence rubric (record this per term)

- **confirmed**: David or a native reviewer signed off. Use freely.
- **high**: macOS and/or Microsoft agree (cite which). Safe to use, still review-gated.
- **tentative**: sources conflict or none had it; your best judgment. Push it to the style guide's "Decisions to confirm
  with David" section rather than burying it.

When sources disagree, weight by tier (macOS > Microsoft > GNOME/Xfce) but note the disagreement — it's often a
macOS-vs-Windows split worth recording for the translator who comes after you.
