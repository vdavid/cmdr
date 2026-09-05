# Decisions waiting on David

**Problem**: a question with no answer looks exactly like a task nobody picked up, so it sits inside a 600-line spec and
keeps that spec alive for years. These are the calls that gate work but are not themselves work. Most take a minute.

Decisions that gate exactly one effort live in that effort's spec instead. A call that gates nothing but is still worth
making sits with the work it came from: `later/idle-cost-follow-ups.md` holds the CLIP idle-unload and compute-unit
calls, and the question of whether the rescan walk may read `SYSTEM_DIR_EXCLUDES`.

## 1. Should one unrecoverable file end the whole operation?

- **Problem**: a file that exhausts its retries ends the operation it belongs to. Copy 700 files, have file 200 fail
  past recovery, and the remaining 500 never move. The alternative is that the batch carries on and reports what it
  could not do.
- **Impact**: the user re-runs the whole operation to rescue the tail, on top of an already slow transfer, and a
  flaky remote (a sleeping NAS, a dropped SMB session) turns one bad file into an abandoned job. No report has named
  this yet, so the frequency is unmeasured.
- **Solution**: a terminal event shape that can say "finished, 500 files copied, 200 missing", a frontend that lists
  which ones and why, and journal semantics for a partially-successful operation (what rollback means when half the
  batch landed). The mechanism is recorded at `transfer/DETAILS.md` § "Not done here".
- **Size**: several days, and it is a product call before it is an engineering one.
- **Clear win or a tradeoff?** A tradeoff. Carrying on rescues the batch, and it also means a user can walk away from a
  finished-looking operation that quietly skipped 200 files, so the honesty of the terminal report is what decides
  whether it is an improvement.
