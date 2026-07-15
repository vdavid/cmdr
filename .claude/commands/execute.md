Lead a team of agents to deliver on this plan.

## Setup

- Work on the worktree where the plan lives (the `plan` command creates one). If the plan was made on the main clone,
  move it to a worktree branched off local `main` first, so your context is preserved for follow-up tweaks. Keep `main`
  clean (see `solo-dev-workflow.md`).

## You (the lead)

- You don't do the implementation work, you oversee the agents. You keep this project together; they do the work. I need
  your context window free for post-implementation checks, fixes, thinking, and the verification below.
- Run agents sequentially, we're in no rush, unless you predict the quality is better in parallel. Look at what each
  agent did between milestones, and feed the previous agent's output into the next one's input.

## Agents

- It's your responsibility that the _whole_ plan gets executed. Agents sometimes skip parts of their scope. Give them a
  clear scope and ask them to do the whole thing. They only say "done" when every part of their job is finished and
  thoroughly self-reviewed.
- Agents sometimes do the opposite: they ignore their milestone boundary and jump on the whole plan. That tanks quality,
  because they run out of context, auto-compress, and the compressed agent loses our values and intent. So give each a
  clear, bounded scope.
- Make every agent reflect: "Is what I've done solid AND elegant? Am I proud and confident about it?" If "no" to either,
  adjust and repeat.
- They should also fix latent bugs near their work (small, ~10-15 LoC changes). Correctness and bug-free code over
  crystal-clean commits.

## Feedback loops are mandatory

Don't fly blind. At checkpoints (and especially after a feature milestone), run the app, read the logs, and drive it via
MCP to confirm new features actually look and feel right. Spawn an agent to run the app, interact with it, and test via
MCP when that's a good use of a fresh context.

- MCP caveat: a freshly spawned Claude Code session often doesn't auto-connect the wired-up MCP even though it's
  configured. Unless you can trigger a refresh, use the CLI fallback `./scripts/mcp-call.sh` (and the Tauri bridge). See
  [docs/tooling/mcp.md](../../docs/tooling/mcp.md).

## Testing and checks

- Cover new features with tests, using real red→green TDD wherever reasonable (see `tdd-red-green.md`).
- Run `pnpm check` (at repo root) after each milestone. Run the slow suite (`--include-slow`) only at the very end.
- When running E2E, run only the set specific to the feature you're working on. Our E2E tests are designed to run well
  under a second each; a focused run is fast.

## Lead verification (you own delegated work)

Don't integrate on trust (see `verify-delegated-work.md`):

- Re-run the security- and data-safety-critical tests yourself.
- Read the actual diffs. Confirm the scope matches the plan's intent: nothing skipped, nothing stray.
- Rebase the worktree onto CURRENT local `main` before the fast-forward merge (it can advance mid-session).

## Keep docs current

Agents keep `CLAUDE.md` files and other docs up to date continuously as they work, so we end in a good documented state,
not with a doc-sync chore at the end.

## Final review

- Ask +1 agent to thoroughly review the execution and flag anything skipped, broken, or incomplete.
- Have +1 agent run `pnpm check` and confirm it's green (even if unrelated checks fail, surface those).
- Strip milestone tags from the touched code and docs. Plan-specific names like "M1", "M2a", "Milestone 3", "Phase 2"
  leak into inline comments, dead-code `reason` strings, test helper prefixes, doc strings, and `CLAUDE.md` text during
  execution. Grep the touched files (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <paths>`) and replace
  each hit with a descriptive reference ("the watcher", "the hook contract", "the downloads toast", etc.) so a future
  reader doesn't need the plan in hand. The plan file itself keeps its milestone structure. Leave pre-existing milestone
  references in unrelated code alone (a different feature's M1, the Apple Silicon "M1" chip, SVG `M` path coords, etc.).
- Do a review yourself, and report: is this something you're proud of? Is this solid AND elegant? Is anything missing?

## Wrap

Fast-forward merge the worktree into local `main`, then delete the worktree and branch (see `solo-dev-workflow.md`).
Don't push, and don't offer to: I push on my own schedule (see `push-cadence.md`). We're done when the work is committed
and we've discussed the follow-ups.
