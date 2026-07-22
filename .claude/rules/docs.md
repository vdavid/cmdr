# Docs: keep them in sync, single-source, and current

When you change code in a directory with a `DETAILS.md` / `CLAUDE.md`, keep the colocated docs in sync (per `AGENTS.md`
§ Docs): `CLAUDE.md` for must-knows, `DETAILS.md` for depth. Add a `Gotcha/Why` when something failed on a wrong
assumption, and a `Decision/Why` to the nearest `DETAILS.md` (plus a one-line `CLAUDE.md` guardrail only if ignoring the
decision can silently break something). Rich evidence (benchmarks, analysis) goes in `docs/notes/` and is linked from
the `DETAILS.md`. Skip all this for trivial changes (formatting, small fixes that don't change architecture).

**Before a sweeping `CLAUDE.md` / `DETAILS.md` slimming or restructuring pass, read `docs/doc-system.md` first** (the
condense-first playbook and the C-vs-D litmus); its own read-trigger otherwise lives only inside it.

**Single-source.** A load-bearing technical claim or mechanism lives in exactly ONE canonical doc (the module doc or
colocated `DETAILS.md` nearest the code); everywhere else points to it by path, never restates it. Copied prose rots
independently. `docs/architecture.md` is a map: what + where + a pointer, never how (no mechanism, flows, or triggers).
This extends `use-codegraph` ("don't transcribe what codegraph owns") from symbol locations to behavioral facts.

**Reference a doc by a bare backticked path** (`` `docs/architecture.md` ``), never a link repeating its own target
(``[`docs/architecture.md`](docs/architecture.md)``): the graph follows both. Link only for descriptive text or an
`#anchor`.

**Current state, not history.** Docs describe the code as it is now; git holds the history. Drop narration of previous
shapes; keep the non-obvious why, actionable guardrails, and historical pain that encodes a constraint the current code
must defend. Full drop/keep lists and code-comment carve-outs: David's user-level `describe-current-not-history` rule.

**Evidence-anchor volatile claims.** Any claim about OS/external behavior, a version, or an empirical finding carries
`(verified on <version/env>, <method>, <date-or-commit>)`. Undated confident claims about drifting behavior turn into
landmines.
