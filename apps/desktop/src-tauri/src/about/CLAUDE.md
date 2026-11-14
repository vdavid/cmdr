# About this (host)

Gathers the "Tell me about this" card (#1104) and streams it over a `Channel<AboutEvent>`. `crates/cmdr-about` owns
every rule; this module reads raw inputs.

## Module map

- `commands.rs` (`about_start`, `about_cancel`, `about_stop_verdict`), `event.rs` (the wire), `registry.rs` (cancel,
  tombstones, quit).
- `route.rs` (resolve, then the `Way` table by volume), `place.rs` (where a local path lives), `probe.rs` (the
  `lstat` before a local plan), `jobs.rs` (a local job per row), `through.rs` (rows asked of a volume, `Stuck`), `session.rs` (plan, grace, budgets, the volume's
  health), `pool.rs` (`about_facts`).
- `platform/`: per-OS seam for xattrs, Spotlight, default app, code signature, `stat` extras.
- `history.rs` (the operation log), `git.rs`, `index.rs` (size, importance), `digest.rs` (What's inside, an app's
  Library data), `image.rs` (What it shows), `owner.rs`, `model.rs` (a model's view, gated), `lookup.rs`.
- `verdict/`: the AI take: `gate.rs` (`model_trust`, the one tier table), `limits.rs`, `peek.rs`, `prompt.rs`, `watched.rs`.

## Must-knows

- ❌ **Route by volume, never by path prefix** (a Finder SMB path looks local). Only a live local disk gets local
  jobs; everything else goes `route::Way` by way, through its OWN calls (`through.rs`).
- ❌ **A volume call runs to its end on a pool thread** (`Handle::block_on`), ❌ never awaited on the executor or cut by
  a bare `timeout` (that wedges an MTP phone). `Stuck` caps a volume at two calls out.
- ❗ **A non-local card is planned from the pane's `isDirectory`, before the volume is asked**, so a dead NAS opens at
  once; its identity call is the health check (`VOLUME_BUDGET`).
- ❗ **`planned` is always first** (or `unavailable` alone), then facts in any order, then `factsSettled`. A kind
  outside the plan never goes out. Verdict events follow `planned` and interleave.
- ❗ **Every `fact` carries `derived` (headline, removal, actions)**, recomputed on landing.
- ❗ **Cancel is `about_cancel`, ❌ never a dropped channel**. The frontend mints the id; a cancel before its start
  leaves a tombstone. It stops the verdict too (a child token).
- ❌ **No framework call on rayon or the executor**: the `about_facts` pool, whose ceiling bounds lost threads.
- ❌ **Cmdr's history credits only the node the log recorded landing** (`landed_at`): a file saved over since is
  `NodeChangedSince`, ❌ never the operation.
- ❌ **Log a fact's kind and outcome, never its value**: logs ride error reports.
- ❌ **A card reaches a model via `model::for_model` alone**, behind a `model::gate` answer.
- ❌ **No call follows a link into a share** (no `statfs`, `canonicalize`): local-ness is `place`'s walk.
- ❌ **`Started::ByCmdr` asks no volume but a local disk** (`RefusedInBackground`); a path linking into a share counts
  as the share (`place::reaches_a_share`).
- ❌ **Git's status list holds CHANGED entries only**: absent is `NotInStatus`, ❌ never "clean".
- ❌ **What's inside, an app's data, and What it shows read databases only** (the index, an ALREADY loaded search
  arena, `media.db`). Not there is `NotIndexed`.
- ❌ **Nothing looks inside an app's container** (`probe::sealed`), and no other volume's app counts as installed.
- ❗ **A signer is named from code requirements, never a certificate name**; a missed check is `NotChecked`.
- ❗ **The verdict's order is the consent guarantee**: gate (provider, setting, slot) → reuse and ceiling → facts →
  sensitive gate and peek → prompt → call. A refusal reads nothing for a model. ❌ Its text is never stored, logged, or
  cached on disk; only its cost lands, on `main.db`'s `about_this` row.
- ❗ **Rerun the whole real-model eval** after changing a verdict prompt, `for_model`, the fence, or a default model: it's
  the only evidence behind the trust tiers (`docs/notes/about-verdict-eval.md`).

Flows, the verdict, and the test map: `DETAILS.md`.
