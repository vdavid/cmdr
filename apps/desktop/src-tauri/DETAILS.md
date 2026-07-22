# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else (architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## One process per data dir

Two Cmdr processes on one data dir corrupt the index (two writers seeding the same entry-ID counter).
`instance_lock.rs` makes that impossible: it takes an advisory `flock` on `<data dir>/.instance.lock` in
the `setup` hook, before any database opens, and exits with a native alert if another process already holds
it. Anything that relaunches the app against a live data dir (an updater path, a capture script, a test
harness) must let the old process exit, or wait out the lock's ~5 s retry window. Mechanism, rationale, and
the retry-window callers: `docs/tooling/instance-isolation.md` § Instance lock.

## Number types over IPC (`ipc.rs`, specta bindings)

Tauri's IPC serializes through JSON, so the generated `bindings.ts` never sees a JS `bigint`.

- **Large integers.** `u64` / `i64` / `usize` / `isize` reach the frontend as TS `number` (`bigint` appears nowhere in
  `bindings.ts`), because a JS `number` truncates above 2^53. The values we actually send (file sizes, byte offsets,
  unix timestamps, counts) stay far below that ceiling, so `number` is honest here. If a command ever needs a genuinely
  huge integer (a raw inode, a hash, a nanosecond epoch), don't lean on this: give the field
  `#[specta(type = String)]` plus a serde `with` and parse it on the frontend.
- **A non-`Option` float can still arrive as `null` at runtime.** `serde_json` serializes `NaN` / `Infinity` /
  `-Infinity` as JSON `null`, so a Rust `f32` / `f64` return value that goes non-finite reaches the frontend as `null`
  even though `bindings.ts` types it `number`. The types don't express this, so keep the value provably finite on the
  Rust side (guard the division, clamp the result) rather than relying on the TS signature. This is a latent hazard,
  not an observed one: no crash or error report has shown a non-finite float crossing IPC.

**Decision: the specta trio stays pinned at `tauri-specta`/`specta` `=2.0.0-rc.24` and `specta-typescript` `=0.0.11`.**
rc.25 types every plain `f32` / `f64` as `number | null`. For *return* values that's arguably more honest (see the
`NaN` note above), but it applies the same rule to *parameters*, where it's simply wrong: `viewer_get_lines`
(`target_value: f64`) and the four `media_index_*_threshold` commands take non-`Option` floats, and serde rejects a
JSON `null` for those. Adopting rc.25 would trade a latent, never-observed hazard for a live one the frontend could
trigger by passing `null`, plus ~25 sites of null-handling. Renovate is disabled on all three (`renovate.json`);
re-evaluate on the next rc and bump all three together or not at all.

`bindings.ts` is generated: change this behavior at the `builder()` call site and regenerate with
`pnpm bindings:regen`, never by hand-editing the output.

The Ask Cmdr bulk-rename review commands register in the same builder and type collector as every other typed command.
Their authority and filesystem behavior live with the agent and write-operation modules, not at this registration edge.
