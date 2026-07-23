Plan the feature implementation we discussed.

1. **Work on a worktree.** Create a worktree branched off local `main` and write the plan there, so execution can run on
   the same worktree with your context preserved (set up per-worktree CodeGraph per `codegraph-worktree.md`). For a tiny
   plan you don't intend to execute as a separate effort, ask first whether a worktree is warranted.
2. Collect context from related `CLAUDE.md` files (and, for an area you're about to change structurally, its colocated
   `DETAILS.md` for the full architecture and decisions) or `docs/`, as needed. Plan with our core product design values
   and design principles front of mind.
3. Save the plan to `docs/specs/{feature}-plan.md` (inside the worktree).
4. Capture the INTENTION behind each decision, not just the steps. The implementing agent or human should know the
   "why"s and be able to adapt dynamically!
5. Use milestones if needed. For each milestone, name the docs updates, the tests that prove it (unit? integration?
   E2E?), and which tests are written test-first as a real red→green sequence (see `tdd-red-green.md`) versus written
   after. Lean TDD for bug fixes and risky logic. Include the checks to run.
6. Leave notes about what can be executed in parallel, but only if it's extremely safe; we're usually not in a hurry and
   sequential running is totally fine.
7. DO NOT enter "Plan mode" unless specifically asked to "Enter plan mode". Use `docs/specs`.
8. Get an agent to review the plan with fresh eyes, and point out any mistakes. Then fix up the plan based on that. Link
   the most crucial docs and design principles to the agent.
9. Do this review round again and again, until the reviewer agent has no meaningful input, or maximum 5 times.
