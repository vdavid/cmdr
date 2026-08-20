# specs/ details

Read this before reorganizing the specs folder or its lifecycle conventions.

- **What lives here**: per-development specs and task lists (plans), indexed in `index.md`. Not a description of
  codebase state; temporary working docs kept for reference, like ADRs.
- **Wipe policy**: this folder gets wiped periodically once each shipped plan's durable intent (feature rationale,
  process) is captured in code or colocated `CLAUDE.md` / `DETAILS.md`. Full statement: `README.md`.
- **`later/`**: deferred work that survives a wipe. Same index discipline; see `later/`.
- **Discipline**: update `index.md` whenever you add or modify a plan, so each stays discoverable.

## Wiping a shipped spec

The wipe is a one-way door for the working tree, so it runs in this order, one spec at a time:

1. **Re-derive the status from the code and `git log`, never from the spec's own status line.** Statuses lag in BOTH
   directions: specs have read "SPECCED, not started" with every phase on `main`, and "the tap adapter is not built"
   after it was. Cited commit hashes are usually dangling, because a branch gets rebased before the FF merge, so verify
   against `file:line` instead.
2. **Move the durable intent into the colocated `CLAUDE.md` / `DETAILS.md` nearest the code**: design decisions and
   their why, guardrails that stop a regression, measured evidence, accepted tradeoffs and lossiness, edge-case
   registers, and gotchas that cost someone time. A recorded "we considered X and said no, because" counts: it stops the
   next agent re-deriving it.
3. **Let the process die**: milestone checklists, sequencing, parallelization notes, "what I checked", per-phase
   correction lists whose substance already landed in the code, and status narration.
4. **Route what's still open to `later/`** rather than keeping the whole spec alive for it, and re-derive any numbers
   the new doc quotes.
5. **Repoint anything citing the spec for CONTENT.** A bare backticked `docs/specs/…` path naming where a decision came
   from is deliberately exempt from `docs-dead-links` and stays.

⚠️ A spec that says "keep this doc for its gotchas" is describing work to do, not an exemption: rehome the gotchas and
wipe it anyway.
