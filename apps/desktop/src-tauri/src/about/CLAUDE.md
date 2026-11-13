# About this (host)

Gathers the "Tell me about this" card (#1104) and streams it over a `Channel<AboutEvent>`. `crates/cmdr-about` owns
the vocabulary and every rule; this module reads raw inputs and hands them there.

## Module map

- `commands.rs` (`about_start`, `about_cancel`), `event.rs` (the wire), `registry.rs` (cancel, tombstones, quit).
- `route.rs` (resolve, then the `Way` table by volume), `probe.rs` (the one `lstat` before a local plan), `jobs.rs` (a
  local job per row), `through.rs` (rows asked of a volume, `Stuck`), `session.rs` (plan, grace, late answers, budgets,
  the volume's health), `pool.rs` (`about_facts`).
- `platform/`: per-OS seam for xattrs, Spotlight, default app, code signature, `stat` extras.
- `history.rs`: Cmdr's own history, one read of the operation log. `git.rs`: the repo, branch, and status.
  `index.rs`: a folder's size and importance, from the index. `digest.rs`: What's inside and an app's Library data.
  `owner.rs`: the owner row's lookups. `model.rs`: what a model reads, behind the gate. `lookup.rs`: the agent's card.

## Must-knows

- ❌ **Route by volume, never by path prefix** (a Finder SMB path looks local). Only a live local disk gets local
  jobs; everything else goes `route::Way` by way, through the volume's OWN calls (`through.rs`).
- ❌ **A volume call runs to its end on a pool thread** (`Handle::block_on`), ❌ never awaited on the executor or cut by
  a bare `timeout` (that wedges an MTP phone). `Stuck` caps a volume at two calls out.
- ❗ **A non-local card is planned from the pane's `isDirectory`, before the volume is asked**, so a dead NAS opens at
  once; its identity call is the health check (`VOLUME_BUDGET`, then `volumeStoppedAnswering`).
- ❗ **`planned` is always first** (or `unavailable` alone), then facts in any order (with at most one
  `volumeStoppedAnswering` among them), then `factsSettled`. A kind outside the plan never goes out; `GATHERED` and
  `through::answers` trim the plan to the rows a job answers.
- ❗ **Every `fact` carries `derived` (headline, removal, actions)**, recomputed on landing; actions by `Way`
  (`route::action_caps`).
- ❗ **Cancel is `about_cancel`, ❌ never a dropped channel** (`send` succeeds after the handler is gone). The frontend
  mints the id; a cancel before its start leaves a tombstone the start honors.
- ❌ **No framework call on rayon or the executor**: the `about_facts` pool. A budget bounds the wait, the ceiling the
  lost threads.
- ❌ **Cmdr's history credits only the node the log recorded landing** (`landed_at` matches inode + birth time): a
  file saved over since is `NodeChangedSince`, ❌ never the operation. It reads the database, never the disk.
- ❌ **Log a fact's kind and outcome, never its value** (URL, sender, path): logs ride error reports.
- ❌ **A card reaches a model via `model::for_model` alone**, behind a `model::gate` answer.
- ❌ **`Started::ByCmdr` asks no volume but a local disk** (`RefusedInBackground`: a wake mustn't wake a NAS).
- ❌ **Git's status list holds CHANGED entries only**: absent is `NotInStatus`, ❌ never "clean". Its walk has its own
  budget (`STATUS_BUDGET`).
- ❌ **What's inside and an app's data read the index and an ALREADY loaded search arena only**
  (`search::run_if_loaded`): no walk, no disk listing, never an arena load from a card. Not there is `NotIndexed`.
- ❌ **The owner row never opens a container and never counts another volume's app as installed**: the app scan is
  cached (rebuilt on `open_with`'s app-change generation), ❌ never per card.
- ❗ **A signer is named from code requirements, never a certificate name**; the executable check has its own budget
  and a miss is `NotChecked(TimedOut)`, ❌ never a pass.

Flows, budgets, the signature and Spotlight evidence, and the test map: `DETAILS.md`.
