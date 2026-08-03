This folder contains notes that are not specs, ADRs, or docs on the system. This is pretty much a catch-all folder for
docs that feel helpful and important for some time, but don't belong anywhere else. Like specs, this folder gets wiped
periodically once we made sure that all important information like intent behind features and processes is captured
somewhere else (code or docs).

Two notes are load-bearing rather than historical, and both are before-and-afters for a crate extraction, including
the method each number was taken with. Keep them: they're what any future "did this get slower?" re-measurement
compares against, and they record what a crate boundary did and didn't buy.

- `index-extraction-baseline.md` — the `cmdr-index` extraction.
- `archive-extraction-baseline.md` — the `cmdr-archive` extraction, which reuses the same scenarios so the two are
  comparable, and which is the measurement gate `docs/specs/backend-crates-plan.md` ends at.
