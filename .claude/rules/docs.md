# Docs: keep them in sync, single-source, and current

When you change code in a directory with a `DETAILS.md` / `CLAUDE.md`, keep the colocated docs in sync (per `AGENTS.md`
§ Docs): `CLAUDE.md` for must-knows, `DETAILS.md` for depth. Add a `Gotcha/Why` when something failed on a wrong
assumption, and a `Decision/Why` to the nearest `DETAILS.md` (plus a one-line `CLAUDE.md` guardrail only if ignoring the
decision can silently break something). Rich evidence (benchmarks, analysis) goes in `docs/notes/` and is linked from
the `DETAILS.md`. Skip all this for trivial changes (formatting, small fixes that don't change architecture).

**Single-source.** A load-bearing technical claim or mechanism lives in exactly ONE canonical doc (the module doc or
colocated `DETAILS.md` nearest the code); everywhere else points to it by path, never restates it. Copied prose rots
independently. `docs/architecture.md` is a map: what + where + a pointer, never how (no mechanism, flows, or triggers).
This extends `use-codegraph` ("don't transcribe what codegraph owns") from symbol locations to behavioral facts.

**Current state, not history.** Docs describe the code as it is now; git holds the history. Drop narration of previous
shapes ("we originally tried X", "no longer applicable as of Z", date-stamped milestone framing). Keep the non-obvious
why, actionable guardrails ("don't switch to X, it breaks Y"), and historical pain that encodes a constraint the current
code must defend. Litmus: if removing the history still leaves current state described AND enough rationale to defend
the code against a "let's clean this up" pass, drop it. David's user-level `describe-current-not-history` rule carries
the full drop/keep lists and the code-comment carve-outs (for code comments, when in doubt, leave it).

**Evidence-anchor volatile claims.** Any claim about OS/external behavior, a version, or an empirical finding carries
`(verified on <version/env>, <method>, <date-or-commit>)`. Undated confident claims about drifting behavior turn into
landmines.
