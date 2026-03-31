When modifying code in a directory that contains a `CLAUDE.md` file, check whether your changes affect the documented
architecture, key decisions, or gotchas. If they do, update the `CLAUDE.md` to stay in sync. If you notice a `CLAUDE.md`
missing in a directory where there should be one, add it. Skip this for trivial changes (bug fixes, formatting, small
refactors that don't change the architecture).

If something failed due to a wrong assumption, add a `Gotcha/Why` entry to the nearest `CLAUDE.md`.

Add `Decision/Why` entries to the nearest colocated `CLAUDE.md` for key decisions. If the decision has rich evidence
(benchmarks, detailed analysis), put the evidence in `docs/notes/` and link from the CLAUDE.md.

When writing guides, see [this diff](https://github.com/vdavid/cmdr/commit/13ad8f3#diff-795210f) for the formatting
standard. (Before: AI-written. After: matching our standards for conciseness and clarity.)
