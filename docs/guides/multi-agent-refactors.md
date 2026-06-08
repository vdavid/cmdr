# Multi-agent refactors

The orchestration loop that shipped the explorer architecture refactor (4 phases, ~40 agents, zero regressions) and the
command-handler-record refactor. Use it for any multi-milestone refactor in Cmdr. `/plan` and `/execute` are the command
entry points; this is the playbook they assume.

## The loop

1. **Plan.** A planner agent writes a just-in-time plan in `docs/specs/`: loud rules, a fresh grep of the current code,
   milestones each carrying Scope / Intentions / Landmines / Test plan / DONE, and an invariants footer. Then fresh-eyes
   review rounds, each a NEW agent coming from a different angle, until one returns no meaningful input (usually 3-4
   rounds). Every round tends to find real bugs, so don't skip them. Templates: `docs/specs/explorer-*-plan.md` and
   `command-handler-record-plan.md`.
2. **Execute.** One Opus agent per milestone, sequential. Each reads the spec itself and reports back in ≤350 words. The
   orchestrator reads every spec in full and reviews every diff (full read for seam commits), and otherwise only
   coordinates: delegate all debugging.
3. **Characterize-then-convert** for risky rewrites. Regression tests that pin CURRENT behavior land as their own
   milestone BEFORE the rewrite. Pin reality, not the plan's expectations, and verify timing-sensitive pins actually
   bite (red first). This makes a byte-identical refactor provable instead of hoped-for.
4. **Gates.** `--fast` continuously; the full suite + `desktop-e2e-linux` per milestone; `--include-slow` at phase end;
   watch CI after a push. Flake policy: isolated-green twice via `e2e-linux.sh --grep` counts as green; a failure in the
   change's own surface is real.
5. **Close out.** An end-of-phase adversarial conformance review agent (against the invariants register) plus a
   docs-audit agent that reads ALL commit bodies and verifies every touched `CLAUDE.md`.

## Why it works

The review rounds catch data-corruption-class design errors before implementation (a per-pane-vs-global counter that
would have frozen the webview, `as const` vs runtime mutation, and `hasParentRow` flattening were all caught pre-code).
The characterization suites turn "I think this refactor is safe" into a proof.
