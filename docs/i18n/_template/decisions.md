# <tag> decisions

The rationale behind this language's term rulings (`terms.json`) and per-feature translation choices, one section per
decision. Translators don't read it whole: `pnpm i18n:brief` pulls the sections whose heading cites a key in the batch.
So every heading cites the keys it's about, in backticks (`` `servers.sheet.remember` ``, `` `servers.*` ``,
`` `a.b.label`/`.description` ``), and a ruling links its section through the term's `decision` field. Format:
`docs/i18n/termbase.md` § `decisions.md` headings.
