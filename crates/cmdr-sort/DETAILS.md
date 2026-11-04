# cmdr-sort details

The three enums a sort request is made of. How a listing applies them (the comparator, directories' placement, the
requested-versus-effective column): `crates/cmdr-listing/DETAILS.md` § "Requested and effective sort".

## Why its own crate

`cmdr-menu` draws View > Sort by's rows, which name a `SortColumn` and a `SortOrder` and nothing else of the listing
engine. When the types lived in `cmdr-listing`, the menu linked the whole engine (tokio, the collator's ICU data, the
fuzzy matcher, and `cmdr-fs` with its bundled SQLite) to name six column words. The same holds for any future crate that
only names a sort.

**Why not `cmdr-fs`.** `cmdr-fs` is the vocabulary of what is ON DISK (entries, volumes, their errors). A sort column is
a pane preference: it names a view of entries, not a fact about one, and `cmdr-fs` would still hand the menu bundled
SQLite, `exacl`, and `xattr` for three enums. `EffectiveSort` stays in `cmdr-listing` for the converse reason: it reads
`cmdr_fs::volume::SupportedDates`, so it belongs with the comparator that takes it.

**Why not `cmdr-collation`.** That crate answers how two NAMES rank, through ICU; a column or a direction isn't a
collation concern, and the menu would link ICU again.

## The public surface is capped

Three root promises, the enums themselves, and nothing else (`scripts/check/checks/index-crate-isolation-ceilings.go`).
A fourth item is vocabulary every sort site sees, so argue it here before raising the ceiling.
