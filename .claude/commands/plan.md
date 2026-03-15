Plan the feature implementation we discussed.

1. Collect context from related `CLAUDE.md` files or `docs/`, as needed.
   Read [design-principles.md](../../docs/design-principles.md)) to remember our core product design values.
2. Save the plan to `docs/specs/{feature}-plan.md`.
3. Capture the INTENTION behind each decision, not just the steps. The implementing agent or human should know the "why"
   s and be able to adapt dynamically!
4. Use milestones if needed. Make sure to include the necessary docs updates roughly, testing (Unit? Integration? E2E?),
   and running all necessary checks.
5. Leave notes about what can be executed in parallel, but only if it's extremely safe and needs no worktrees; we're
   usually not in a hurry and sequential running is totally fine.
6. DO NOT enter "Plan mode" unless specifically asked to "Enter plan mode". Use docs/specs.
7. Get an Opus agent to review the plan with fresh eyes, and point out any mistakes. Then fix up the plan based on that.
   Link the most crucial docs and design principles to the agent.
8. Do this review round again and again, until the reviewer agent has no meaningful input, or maximum 5 times.