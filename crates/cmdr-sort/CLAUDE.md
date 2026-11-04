# cmdr-sort

What a pane asks to be sorted by: `SortColumn`, `SortOrder`, and `DirectorySortMode`. Vocabulary only, with serde and
specta and nothing else. `cmdr-listing` re-exports all three beside the comparator that applies them, so the app names
them from there; `cmdr-menu` imports them from here so the menu bar links no listing engine.

## Module map

- **lib.rs**: the whole crate. The three enums and the tests that pin their wire words and defaults.

## Must-knows

- **The wire words are the frontend's** (`camelCase`): the persisted sort settings, every IPC command that names a sort,
  and `bindings.ts` read them. A rename is a wire break; `the_wire_words_are_the_frontends` pins them.
- **Ordering rules don't live here.** The ONE comparator, and the fallback from a date a volume doesn't record, are
  `cmdr-listing`'s (`entry_comparator`, `EffectiveSort`). ❌ No `impl` that sorts or resolves.
- **The specta names are what `bindings.ts` exports**; moving a type between crates keeps it byte-identical, renaming
  one doesn't.
- The public surface is capped by `index-crate-isolation`, and no `tauri` is allowed in the tree.

Why its own crate, and why not `cmdr-fs`: `DETAILS.md`.
