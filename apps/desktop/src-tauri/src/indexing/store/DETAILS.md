# Index store (SQLite) details

Depth for `src-tauri/src/indexing/store/`: the `IndexStore` handle and the concern-split CRUD. Must-know invariants
live in [CLAUDE.md](CLAUDE.md). The SQLite schema itself and the honest-sizes epoch model that shares its columns are
one coupled mechanism and live in the parent [`../DETAILS.md`](../DETAILS.md) § "SQLite schema" + § "Honest sizes"; the
broader indexing pipeline is the rest of that file.

## Module structure

The `IndexStore` read/write handle and SQLite schema, split into a `store/` submodule by concern. `mod.rs` holds the
shared core: the schema (integer-keyed entries with `name_folded` on all platforms, `inode` for hardlink dedup,
`dir_stats` by entry_id, `meta`), `platform_case` collation, DDL/pragmas/reset, the path helpers (`resolve_path`,
`reconstruct_path*`), the `IndexStore` struct + `with_savepoint`, and the data types (`EntryRow`, `DirStats`,
`DirStatsById`, `ScanContext`, `IndexStatus`, `ScanCalibration`, `IndexStoreError`); the `tests` module lives in the
sibling `tests.rs`.

The `impl IndexStore` block is divided into four sibling files (each `impl IndexStore { … }` over the struct above,
pulling shared items via `use super::*`):

- `connection.rs`: open/recreate, connection factories, DB-size + status reads, the `pub(super)` `read_meta_value`
  helper.
- `entries.rs`: entry-tree reads and writes — child listings, lookups by id / inode / component, insert / update /
  rename / move / delete, counts, `get_next_id`.
- `dir_stats.rs`: `dir_stats` reads and writes plus `recompute_min_subtree_epoch`.
- `meta.rs`: meta-table + epoch helpers, `mark_dirs_listed`, `get_all_directory_paths`, `clear_all`.

`resolve_component` always queries by `(parent_id, name_folded)` using the `idx_parent_name_folded` composite **UNIQUE**
index. On Linux/Windows `normalize_for_comparison()` is the identity function, so `name_folded = name` and the index
behaves identically to a `(parent_id, name)` index. A schema-version mismatch triggers drop+rebuild.
`has_sized_entry_for_inode()` checks whether another entry with the same inode already has non-NULL sizes;
`find_entry_by_inode()` returns the first row with a given inode (the live event loop's rename pre-pass). Both path-keyed
(backward compat) and integer-keyed APIs exist.
