When modifying code in a directory that contains a `CLAUDE.md` file, check whether your changes affect the documented
architecture, key decisions, or gotchas. If they do, update the `CLAUDE.md` to stay in sync. If you notice a `CLAUDE.md`
missing in a directory where there should be one, add it. Skip this for trivial changes (bug fixes, formatting, small
refactors that don't change the architecture).

If something failed due to a wrong assumption, add a `Gotcha/Why` entry to the nearest `CLAUDE.md`.

Add `Decision/Why` entries to the nearest colocated `CLAUDE.md` for key decisions. If the decision has rich evidence
(benchmarks, detailed analysis), put the evidence in `docs/notes/` and link from the CLAUDE.md.

When writing guides, see [this diff](https://github.com/vdavid/cmdr/commit/13ad8f3#diff-795210f) for the formatting
standard. (Before: AI-written. After: matching our standards for conciseness and clarity.)

## Describe current behavior, not history

`CLAUDE.md` files describe the current state of the code and app; git history is for history. Drop narration of previous
code shapes ("we originally tried X", "no longer applicable as of Z", date-stamped milestone framing on Decisions). Keep
the non-obvious why, actionable guardrails ("don't switch to X, it breaks Y"), and historical pain that encodes a
constraint the current code must defend. Litmus: if removing the history still leaves current state described AND enough
rationale to defend the code against a "let's clean this up" pass, drop it. (David's user-level
`describe-current-not-history` rule carries the full drop/keep lists and code-comment carve-outs; for code comments,
when in doubt, leave the comment.)
