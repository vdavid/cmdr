# How to mine the reference pile

Tested recipes for extracting a term or convention from each source in `<tag>/`. The point: don't read whole files or
reinvent the search each time. Triangulate a term across every source the language has, then record your choice with
sources and a confidence in the per-language style guide (`docs/i18n/<tag>/style.md`). Structure and provenance of the
pile: [README.md](README.md) and [inventory.md](inventory.md).

First check which sources your language has: `ls <tag>/` (a source is absent if its subdir is missing). Run everything
below from `_ignored/i18n/`.

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

## Confidence rubric (record this per term)

- **confirmed**: David or a native reviewer signed off. Use freely.
- **high**: macOS and/or Microsoft agree (cite which). Safe to use, still review-gated.
- **tentative**: sources conflict or none had it; your best judgment. Push it to the style guide's "Decisions to confirm
  with David" section rather than burying it.

When sources disagree, weight by tier (macOS > Microsoft > GNOME/Xfce) but note the disagreement — it's often a
macOS-vs-Windows split worth recording for the translator who comes after you.
