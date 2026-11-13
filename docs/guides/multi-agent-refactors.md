# Multi-agent refactors

The orchestration loop that shipped the explorer architecture refactor (4 phases, ~40 agents, zero regressions) and the
command-handler-record refactor. Use it for any multi-milestone effort in Cmdr.

## The loop

1. **Plan.** A planner agent writes a just-in-time plan in `docs/specs/`: loud rules, a fresh grep of the current code,
   milestones each carrying Scope / Intentions / Landmines / Test plan / DONE, and an invariants footer. Then fresh-eyes
   review rounds, each a NEW agent coming from a different angle, until one returns no meaningful input (usually 3-4
   rounds). Every round tends to find real bugs, so don't skip them. For template examples, read a past refactor plan in
   git history (the explorer phase plans and `command-handler-record-plan.md`, since wiped from `docs/specs/`).
2. **Execute.** One agent per milestone, sequential. Each reads the spec itself and reports back in ≤350 words. The
   orchestrator reads every spec in full and reviews every diff (full read for seam commits), and otherwise only
   coordinates: delegate all debugging.
3. **Characterize-then-convert** for risky rewrites. Regression tests that pin CURRENT behavior land as their own
   milestone BEFORE the rewrite. Pin reality, not the plan's expectations, and verify timing-sensitive pins actually
   bite (red first). This makes a byte-identical refactor provable instead of hoped-for.
4. **Gates.** The cadence in `AGENTS.md` § "Checker script", plus `desktop-e2e-linux` on a milestone that touches its
   surface, plus green CI on a `pnpm land -no-wait` push. Flake policy: isolated-green twice via `e2e-linux.sh --grep`
   counts as green; a failure in the change's own surface is real.
5. **Close out.** An end-of-phase adversarial conformance review agent (against the invariants register) plus a
   docs-audit agent that reads ALL commit bodies and verifies every touched `CLAUDE.md`.

Lead's guardrails, learned the hard way:

- **Plans capture intent, not just steps**, so an executing agent can adapt when the code disagrees with the plan.
- **Give each agent a clear, bounded scope** and ask for the whole of it: agents both skip parts of their scope and run
  past their milestone into the next one. Have each one ask itself "is this solid AND elegant?" before reporting done.
- **Strip milestone tags before merging.** "M1", "Milestone 3", "Phase 2" leak into comments, `reason` strings, test
  helper names, and `CLAUDE.md` text; `rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <touched paths>` finds
  them, and each becomes a descriptive name ("the watcher", "the hook contract"). The plan file keeps its own.
- **Only the lead lands** (`scripts/land/CLAUDE.md`): a subagent can't wait out CI.
- **Land ready branches as one batch, not one by one.** Cherry-pick every branch that's ready onto one fresh worktree
  from `origin/main`, resolve conflicts once, run the checks the merge touched, and `pnpm land` that. Why: each land run
  holds the macOS runner (mostly the M1's four slots, `docs/tooling/ci.md`) and many Linux ones for its whole run
  (`pnpm land eta` prints how long that is now), and each separate land moves `main` under the others, so they rebase
  and rerun CI (one branch ran CI three times on 2026-10-28). Close the batched PRs, and remove their worktrees with
  `remove-worktree.sh --force` once the batch is on `main`.
- **Parallel agents that edit code each get their own worktree** with `isolation: "worktree"`: the harness's
  `WorktreeCreate` hook makes it warm through `new-worktree.sh`, and the agent is pinned there. It can't join an
  existing worktree, and nothing removes it for you: the lead does, once its branch has landed.
- **A worktree-isolated agent runs `git` as plain, separate commands from its own worktree.** Claude Code itself (not a
  hook of ours) refuses a `git` it can't prove stays inside the worktree: one inside `<(...)`, a loop, or after a `cd`
  to a path computed at runtime ("names git in a form too complex to verify", seen 2026-11-12). Nothing here can relax
  it, so split the command up; to read another branch, name it (`git show <branch>:<path>`) from the agent's own
  worktree.

## Fan-out sweeps (parallel research or generation over a list)

A different shape from the refactor loop above: many independent agents each handle a slice of a long list (e.g. one doc
per language). Hard-won rules:

- **Pilot ONE agent before fanning out.** Run it alone, then read what it built and what it worked around (its report's
  Process feedback, and the scripts it left in the scratchpad), fix the tooling or the brief, and only then start the
  rest. **Why:** 13 translators each rebuilt a catalog write-back script on 2026-11-12 (about 850 lines in all) and none
  reported it, because to an agent a script is ordinary work and no single agent can see the pattern.
- **After the first wave, look for homemade tooling.** List the scratchpad (and each agent's Process feedback, which
  translators log in `docs/i18n/source-queue.md`): two agents writing the same kind of script is a missing tool. Build
  it before the next wave, rather than letting every agent pay for it again.
- **Cap concurrency at ~3 subagents.** The Anthropic API rate-limits aggressively; a 14-agent burst gets throttled and
  agents silently finish partial. Keep ~3 in flight; queue the rest. Batch size per agent (3 vs 10 items) is separate
  and can stay large once concurrency is capped.
- **Enter the worktree before spawning, and give ABSOLUTE write paths.** Subagents inherit the orchestrator's worktree
  pin and cwd, so an orchestrator that called `EnterWorktree` puts every agent on the worktree with no `EnterWorktree`
  of their own. One still on the main clone leaves them there, where a relative write path scatters into the wrong tree.
- **Terse returns, artifacts to files.** Tell agents to write their output to files and reply with one line per item
  (plus flags), not a full report. Verbose returns blow the orchestrator's context over many waves. Persist a
  progress/plan file so the loop survives a context compaction.
- **Bake guardrails into the prompt.** Anti-hallucination (ground claims in local sources, mark web-only as unverified),
  format rules, and "don't emit literal tool-call tags" prevent a cleanup pass later.
- **Batched meta-check round after.** Independent reviewer agents (alphabet thirds) spot-check claims against ground
  truth and flag stubs; this is what catches the fabrication-class errors the generators miss.

## Why it works

The review rounds catch data-corruption-class design errors before implementation (a per-pane-vs-global counter that
would have frozen the webview, `as const` vs runtime mutation, and `hasParentRow` flattening were all caught pre-code).
The characterization suites turn "I think this refactor is safe" into a proof.
