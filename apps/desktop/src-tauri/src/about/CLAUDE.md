# About this (host)

Gathers the "Tell me about this" card (#1104) and streams it over a `Channel<AboutEvent>`. `crates/cmdr-about` owns
the vocabulary and every rule; this module reads raw inputs and hands them there.

## Module map

- `commands.rs` (`about_start`, `about_cancel`), `event.rs` (the wire), `registry.rs` (cancel, tombstones, quit).
- `route.rs` (resolve, then reach by volume), `probe.rs` (the one `lstat` before the plan), `jobs.rs` (a job per row),
  `session.rs` (plan, grace, late answers, budgets), `pool.rs` (`about_facts`).
- `platform/`: per-OS seam for xattrs, Spotlight, default app, code signature, `stat` extras.
- `history.rs`: Cmdr's own history, one read of the operation log. `git.rs`: the repo, branch, and status.
  `index.rs`: a folder's size and importance, from the index. `digest.rs`: What's inside and an app's Library data.
  `owner.rs`: the owner row's lookups.

## Must-knows

- ❌ **Route by volume, never by path prefix.** Only a live local volume whose mount is a local disk gets local jobs;
  anything else answers identity through `Volume::get_metadata` under a deadline. A Finder SMB path looks local.
- ❗ **`planned` is always first** (or `unavailable` alone), then facts in any order, then `factsSettled`. A kind outside
  the plan never goes out; `GATHERED` trims the plan to the rows a job answers.
- ❗ **Every `fact` event carries `derived` (headline + removal)**, recomputed on landing: the card never derives them.
- ❗ **Cancel is `about_cancel`, ❌ never a dropped channel** (`send` succeeds after the handler is gone). The frontend
  mints the id; a cancel before its start leaves a tombstone the start honors.
- ❌ **No framework call on rayon or the executor**: jobs and the probe run on the `about_facts` pool. A budget bounds
  the wait, the pool ceiling bounds lost threads.
- ❌ **Cmdr's history credits only the node the log recorded landing** (`landed_at` matches inode + birth time): a
  file saved over since is `NodeChangedSince`, ❌ never the operation. It reads the database, never the disk.
- ❌ **Log a fact's kind and outcome, never its value** (URL, sender, path): logs ride error reports.
- ❗ **xattrs are read capped** (`read_capped`: size first, refuse past 64 KB). An online-only file plans no byte read.
- ❌ **Git's status list holds CHANGED entries only**: absent is `NotInStatus`, ❌ never "clean". The walk has its own
  budget inside the row (`STATUS_BUDGET`); the root and branch stand without it.
- ❌ **What's inside and an app's data read the index and an ALREADY loaded search arena only**
  (`search::run_if_loaded`): no walk, no disk listing, never an arena load from a card. Not there is `NotIndexed`.
- ❌ **The owner row never opens a container and never counts another volume's app as installed**: the app scan is
  cached (rebuilt on `open_with`'s app-change generation), ❌ never per card.
- ❗ **A signer is named from code requirements, never a certificate name**; the executable check has its own budget
  and a miss is `NotChecked(TimedOut)`, ❌ never a pass.

Flows, budgets, the signature and Spotlight evidence, and the test map: `DETAILS.md`.
