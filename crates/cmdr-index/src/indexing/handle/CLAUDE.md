# The index handle

`Index` is the index's public API. Everything the app can ask the three index subsystems for is a method here; nothing
else is reachable from outside. It's what `indexing/mod.rs` re-exports, and at the extraction it becomes the crate's
front door.

## Must-knows

- **A `pub` here is a promise. Adding one is a design act, not a compile fix.** Before you add a method, check the four
  dispositions in `DETAILS.md` § "The public surface": name it for what the caller wants (never for the internal behind
  it), fold it into a call that already exists, delete it, or put it behind the `testing` feature. The surface is 37
  items and each one is justified in that table; the next one needs the same. Both raises are SPENT (`cover`, the
  coverage concept's walk half; `disk_footprint` + `forget_all_volumes`, what the index occupies on disk and dropping
  all of it), so there is no headroom left.
- **❌ The app never calls into `indexing::` internals.** It holds the handle (`crate::index_host::index()`) and calls
  methods. A `crate::indexing::<area>::…` from app code is a back-edge that stops compiling at the extraction, so it's a
  bug now, not later.
- **Building twice is `IndexBuildError::AlreadyBuilt`, and that's honest.** The subsystems below the handle carry
  process-wide state, so there is one index per process. The variant disappears when that state moves inside; don't
  paper over it by handing back a second handle.
- **A test gets a handle through `IndexBuilder::install_for_test`, never `build`.** It installs the seams under a
  restore-on-drop guard and leaves the process's claim alone, so the next test in the binary can still build. Hold
  `handle::test_lock()` first: the seams are process-wide.
- **`observe_listing` and `size_of` are designed, not implemented.** Their bodies report `NotImplemented`. ❌ Never
  `todo!()` here — the crate root denies it. The types reserve three things that are painful to retrofit; `ingest.rs`
  says which and why, and changing those shapes is the expensive kind of change.
- **Errors are typed.** `IndexError::Internal(Diagnostic)` is the residue for causes no caller acts on yet, and its
  payload is log-only. ❌ Never branch on it; a cause worth handling gets a variant.

## Module map

- `mod.rs` — `Index` and its methods, grouped: turning volumes on and off, what it knows about a volume, serving what it
  indexed, coverage (what it can't answer for yet), reading the database directly, and corrections from the host.
- `builder.rs` — `IndexBuilder`, the process claim, and the test install path.
- `error.rs` — `IndexError`. `ingest.rs` — the designed-not-implemented write side plus its types.
- `tests.rs` — the single-instance contract, and the acceptance scan that drives a real walk over an `InMemoryVolume`
  with no app types in the room.

The item-by-item audit that decided the surface, and why the two exceptions are exceptions: `DETAILS.md`.
