# <tag> decisions

The distilled rulings behind this language's term choices (`terms.json`) and per-feature translation choices, one
section per decision: "X over Y because Z", at most ~3 lines. When a ruling changes, edit or replace its section and
delete what it supersedes; never append a narrative or a dated story. A byte budget that only shrinks holds the file to
that (`i18n-termbase`). Translators don't read it whole: `pnpm i18n:brief` pulls the sections whose heading cites a key
in the batch. So every heading cites the keys it's about, in backticks (`` `servers.sheet.remember` ``,
`` `servers.*` ``, `` `a.b.label`/`.description` ``), and a ruling links its section through the term's `decision`
field. Format: `docs/i18n/termbase.md` § `decisions.md` headings.
