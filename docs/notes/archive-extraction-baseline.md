# Archive-extraction: the measurement gate, and why it is still open

The build-time before-and-after for moving `backends/archive/**` (8,352 lines, 2.5% of `src-tauri/src`) into the
standalone `cmdr-archive` crate. This is the measurement gate the backend-crate extraction ended at; the seam rationale,
and what a crate boundary does and doesn't buy, is `crates/cmdr-fs/src/volume/host/DETAILS.md`.

> **⚠️ The gate has NOT been measured cleanly, and nothing here should be acted on as if it had.** Every timing below
> was taken on a machine running several concurrent workspace builds (load average 27–125) on a data volume that was
> near-full and hit 100% shortly afterwards. A near-full APFS volume changes allocation behavior, so this isn't even a
> constant-factor slowdown you could reason around. **The re-take procedure is at the bottom; run it before quoting a
> number.**
>
> Two readings survive the contention as ORDER-OF-MAGNITUDE claims, because a 10–14× gap in CPU seconds can't be
> manufactured by load, and because the structural reason for them isn't a measurement at all. They're marked below.
> Everything else is withdrawn.

Scenarios and commands match `docs/notes/index-extraction-baseline.md`, which was taken properly (idle machine, load ~8
at worst) and is the model this one should be re-taken to.

## What survives the contention

**The scoped inner loop dropped by roughly an order of magnitude, and this is safe in direction and scale.** It's the
one thing the crate boundary was built to buy.

| scenario                                                      | before (CPU s) | after (CPU s) | reading           |
| ------------------------------------------------------------- | -------------- | ------------- | ----------------- |
| Archive edit, then `cargo check --lib` on what you're editing | ~4.7           | ~0.34         | ~14×, provisional |
| Archive edit, then `cargo test --lib --no-run` on it          | ~27.6          | ~2.7          | ~10×, provisional |

**Why these two survive when the others don't.** They're CPU seconds (user + sys, children included), not wall clock, so
they measure work done rather than time spent waiting behind other agents. And the structural reason is independent of
any measurement: `cargo check -p cmdr-archive` compiles ~8k lines plus `cmdr-fs`, where the same question used to
compile the 332k-line app crate, because before the split there was no way to ask for less. A 14× gap is consistent with
that; contention could plausibly move it to 10× or 20×, not to 1×.

**Treat the multipliers as "roughly ten to fifteen times", never as −93% / −90%.** The precision isn't there.

**Two absolute facts about the extracted state that need no before-comparison and no quiet machine:**

- `cargo test -p cmdr-archive` builds and runs all 146 of the crate's tests in **0.8 s**.
- The release binary grew from **79,792,240 to 80,100,336 bytes (+0.4%)**. Output size is deterministic — load and disk
  pressure don't change the bytes — so this one is simply a fact, and it's the direction thin LTO already moved things.

## What is withdrawn

- **"A full app build after an archive edit is flat."** The samples were 18.07 CPU s before against 16.27 and 19.21
  after. That spread is smaller than the contention noise, so the honest statement is that this scenario is
  **unmeasured**, not that it's flat. It's the row where the index extraction found its one regression (+11%), so it's
  the row most worth re-taking properly.
- **The entire release-build comparison** (a −13% wall-clock reading, and a CPU-utilization figure offered as its
  mechanism). One run per side, load 78–125, on a nearly-full volume, during a build that writes gigabytes. A
  utilization number under variable external load describes the competitors more than it describes cargo. Withdrawn in
  full; only the binary size above survives.

## What the gate's answer does NOT depend on

Worth stating plainly, because it means the plan isn't blocked on the re-take: **the recommendation to write new
backends as crates and to hold `cmdr-smb` doesn't rest on any number here.** It rests on a cost asymmetry taken from the
survey, not the stopwatch:

- Archive's whole coupling was three seams, no `cfg(test)` behavior gates, no Docker, no Tauri types.
- SMB's is 23 sites across all seven seams, an `AppHandle` in a `OnceLock` feeding `tauri_specta` emits, two registry
  reach-backs, a `pub(in crate::…)` visibility with no cross-crate spelling, 5,343 lines of Docker-gated tests reached
  through a `use super::*` prelude glob, and a `smb2 = { features = ["testing"] }` forward.
- A backend written as a crate from day one pays almost none of that and gets the same inner-loop win, because that win
  comes from not compiling the app rather than from how much code moved.

So: **P4 unconditionally; `cmdr-smb` only when someone is about to spend sustained time inside SMB.** A clean re-take
could strengthen or weaken the SIZE of the benefit, but it would have to invert the inner-loop result entirely — not
merely shrink it — to change that ordering.

One measured argument in P3's favor that the plan didn't anticipate, and that no timing affects: the extraction surfaced
two latent defects the app crate had been hiding (seven `.unwrap()`s legal only because their file was `cfg(test)`, and
a rustdoc link to a function that no longer exists). Neither was caught by any check while the code lived in the app.
SMB's test surface is 1.6× archive's.

## Re-taking this properly

**Preconditions, all of them.** The whole point of a gate is that it's a number worth acting on:

- Load average under ~5, and no other agent building this workspace. Check `uptime` at the start AND end of each side,
  record both, and discard the run if it moved much.
- At least 40 GB free on the data volume (`df -h /`), so a release build never runs against a near-full APFS.
- ❌ Don't take wall-clock-only numbers on a shared machine. Record CPU seconds alongside, and say which one a
  conclusion rests on.

**The two sides.** `before` is `152d3fe79`, the commit immediately preceding the move, where the archive backend already
talks to its host through the seams but still lives in the app crate. ❌ Not `main` — that would charge the seam work
(P0, P1, and the seam rewiring) to the extraction and measure the wrong thing. Check the two out in the same worktree,
minutes apart, and warm each side with a full build before timing anything.

**The edit** is a real one-line change (a changed log-message string in the archive content watch), never a `touch`, so
incremental compilation has to redo codegen. Flip it back and forth between samples. Take five samples and use the
median; the first sample of each scenario is a cold outlier the median discards.

```bash
# --- after (on worktree-david-backend-crates) ---
cargo check -p cmdr-archive --lib
cargo test  -p cmdr-archive --lib --no-run
(cd apps/desktop/src-tauri && cargo build)          # whole app, after an archive edit

# --- before (at 152d3fe79) ---
(cd apps/desktop/src-tauri && cargo check --lib)
(cd apps/desktop/src-tauri && cargo test --lib --no-run)
(cd apps/desktop/src-tauri && cargo build)

# --- release, once per side, from the repo root ---
cargo clean --release && cargo build --release
ls -l target/release/Cmdr
```

Time each with bash's `time` keyword under `TIMEFORMAT='%3R %3U %3S'` (wall, user, sys) rather than `/usr/bin/time -p`:
the latter writes its report to the same stderr you want to discard from the build, and silently yields nothing.

**Thin LTO must be on for both sides** (`[profile.release] lto = "thin"` at the workspace root), as it was for the index
measurement. It already is; just don't toggle it while comparing.
