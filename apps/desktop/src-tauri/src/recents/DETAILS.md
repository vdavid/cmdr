# Recents details

Depth and rationale. `CLAUDE.md` holds the must-knows.

## What the abstraction is, and where its edge is

`RecentsFile<E>` is "a list of recent things that survives a restart": newest first, one row per distinct thing, a cap
from the oldest end, one JSON file. `RecentEntry` asks a consumer for exactly the four facts that vary between the three
lists, and nothing else:

- `FILENAME` / `LOG_TARGET` / `LOG_NAME`: which file, and how the list names itself in a log line.
- `SCHEMA_VERSION`: defaulted to 1; each list versions its own file, so one can bump without touching the others.
- `id()` / `set_id()`: the id the frontend removes by. `set_id` exists for one job: when a caller hands over an id the
  list already holds, the store assigns a fresh uuid so two rows can't share one.
- `dedupe_key()`: what makes two entries the same thing. Search and Selection build a canonical key out of mode, query,
  filters, and flags; Go to path uses the resolved path string.

The edge is deliberate: the store never learns what an entry MEANS. It doesn't know about modes, filters, or paths, and
it doesn't own a cap. That's what keeps a change to one dialog's semantics from reaching the other two.

### Decisions

- **The cap is an argument, not state.** Search and Selection read `*.maxCount` from settings on every add and live-apply
  a change through `apply_max_count`; Go to path passes `MAX_RECENTS`, a const tied to the dialog's ten digit keys. A cap
  stored on `RecentsFile` would have made the fixed-cap list carry the tunable list's knobs, which is the specific
  coupling the earlier clone-and-trim decision was avoiding.
- **Entry structs stay per-feature.** `HistoryEntry`, `SelectionHistoryEntry`, and `RecentPathEntry` are three separate
  on-disk schemas in three separate files. Collapsing them into one shape (or one file with a `kind` discriminator) would
  bind three independent migrations together forever, and the wire shapes genuinely differ: only Search has `scope` and
  `exclude_system_dirs`, only Go to path has `path`.
- **`RecentsFile::new()` is `const`, so a consumer needs no `OnceLock`.** `Mutex::new` and `Vec::new` are both const, so
  the list is a plain `static`.
- **The in-memory list is a `Vec<E>`, not the envelope.** The schema version in memory was always the current one:
  `persistence::read` returns entries only when the version matches, and quarantines otherwise. Keeping a field that can
  hold exactly one value invites code that re-normalizes it on every write.

## Concurrency

Two locks, and no code path holds both:

- `cached: Mutex<Vec<E>>` — the list. Every mutation goes through `update`, which takes the guard, applies the change,
  clones an owned snapshot, and drops the guard before anything touches the disk. A caller can't hold it across an `fs`
  call because it never has it.
- `disk: Mutex<()>` — serializes the read-modify-write cycle, so two IPC commands landing together can't clobber each
  other's file.

`load_at` is the one place that takes both, and it releases the disk guard before taking the cache one; that's also the
order that would deadlock against `update` if the two ever nested, which is the reason neither does.

Both use `lock_ignore_poison()`: the data is a plain list where any single operation leaves it well-formed, which is the
recover case in `crate::ignore_poison`'s policy.

## Testing

`RecentsFile`'s operations come in two layers, and the split is what makes them testable:

- The path-driven core (`load_at`, `add_at`, `remove_at`, `clear_at`, `apply_max_count_at`) takes the file location as an
  argument, so a test drives the whole cycle against a tempdir with no `AppHandle`. A `None` path is the real "the app
  data dir didn't resolve" case: the list still moves, the write is skipped.
- The app-bound facade (`load`, `add`, `remove`, `clear`, `apply_max_count`) resolves the path and delegates. It's the
  only Tauri-aware code in the module.

Consumers construct their own `RecentsFile::new()` in tests rather than reaching for the shared `static`, so tests don't
interfere through process-global state.

Collapsing the three copies onto this type gave all three lists coverage only some of them had: the zero-cap and
`trim_to` cases (Go to path had neither), and, new to all three, that an add/remove/clear/cap-change actually reaches
disk, that a no-op remove and a cap that drops nothing skip the write, that `entries(limit)` truncates, that a write
creates its parent dir, and that a second corruption replaces the first quarantined copy.
