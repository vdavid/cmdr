When doing legwork: help your team deliver on the plan provided.

## General

- Use **feedback loops**! Never fly blind. You should use benchmarks for optimization tasks, run the app at checkpoints
  like at end of milestones, read the logs, drive app via MCP, or whatever that matches the task to get feedback.
  - Caveat: sessions often don't auto-connect the wired-up MCP even though it's configured. If that happens, use the CLI
    fallback `./scripts/mcp-call.sh` (and the Tauri bridge). See [mcp.md](../tooling/mcp.md).
- Also fix latent, **unrelated bugs** near your work (small, ~10-15 LoC changes) if you discover them. Same goes for
  improving stale docs and the such. We love correctness and bug-free code more than crystal-clean commits/worktrees.
- Keep `C+D.md` files and other docs up to date continuously as you work, so we end in a good documented state.
- Always feel encouraged to improve Cmdr's MCP! If you feel that a feature is missing, or is unintuitive, improve it!

## Wrapping

- Cover new features with tests, using real red→green TDD wherever reasonable.
- Confirm your stuff looks and feels right before wrapping.
- Make sure all docs are updated so future agents can maintain your code.
- Run `pnpm check -q` after each milestone, before wrapping. You may also run the slow suite (`--include-slow`) if you
  need it to feel confident about your stuff. Alternatively, run only the set specific to the feature you're working on.
  Our E2E tests are designed to run well under a second each; a focused run is fast. Include in your final report what
  checks you ran and the final result.
- If you did benchmarking, return interesting results in your response.
- If you get a warning about a file you touched having grown over its allowed size, and you feel it could genuinely
  improve the architecture if you split it, split it. Otherwise, leave it.
- **Reflect** at the end of your work: "Is what I've done solid AND elegant? Am I proud and confident about it?" If "no"
  to either, adjust and repeat.
