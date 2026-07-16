Lead a team of agents to deliver on this plan.

**You** are the lead. Don't do the legwork, oversee the agents and keep the project together. Your context is needed for
follow-up tweaks.

## Before delegation

- Work on the **worktree** provided, or branch off local `main`. If the plan is on `main` uncommitted, move it to the
  worktree.
- It's your choice between Opus, Sonnet, and Haiku agents. Probably don't spawn a Fable agent for legwork.
- Run agents **sequentially**, we're usually in no rush. Look at what each agent did between milestones, feed takeaways
  into the next agent's input.
- It's your responsibility that the **whole plan** gets executed. Agents sometimes skip parts of their scope. Give them
  a clear scope and ask them to do the whole thing. They should only say "done" when every part of their job is finished
  and thoroughly self-reviewed.
- Agents sometimes do the opposite: they ignore their milestone boundary and jump on the whole plan; also bad. So give
  each a clear, **bounded scope**.
- Make every agent **reflect**: "Is what I've done solid AND elegant? Am I proud and confident about it?" If "no" to
  either, adjust and repeat.
- When spawning subagents, link them `@docs/guides/agent-legwork-guide.md` so they know how to be a helpful team member.
  It encourages using feedback loops, doing TDD, updating docs, and running the checker script. It also allows them to
  fix unrelated bugs, improve docs, improve Cmdr MCP, and split large files.

## After delegation

- You're responsible for what we're shipping. You are not _required_ to read all actual diffs, but do check it to the
  extent you need to feel confident about our work. But generally, Opus, and even Sonnet agents do pretty solid work, so
  just relying on their reports is usually enough.
- Confirm the delivered scope matches the plan's intent: nothing skipped, nothing stray.
- In the end of a bigger project, spawn an agent to run the app, interact with it, and test via MCP. Good to have fresh
  eyes on it.
- Rebase the worktree onto CURRENT local `main` before the FF-merge (it can advance mid-session).
- Ask +1 agent to thoroughly review the execution and flag anything skipped, broken, or incomplete, and run
  `pnpm check -q --include-slow`, and confirm it's green (even if unrelated checks fail, surface those).
- Strip milestone tags from the touched code and docs. Plan-specific names like "M1", "M2a", "Milestone 3", "Phase 2"
  leak into inline comments, dead-code `reason` strings, test helper prefixes, doc strings, and `CLAUDE.md` text during
  execution. Grep the touched files (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <paths>`) and replace
  each hit with a descriptive reference ("the watcher", "the hook contract", "the downloads toast", etc.) so a future
  reader doesn't need the plan in hand. The plan file itself keeps its milestone structure. Leave pre-existing milestone
  references in unrelated code alone (a different feature's M1, the Apple Silicon "M1" chip, SVG `M` path coords, etc.).
- Do a review yourself, and report: is this something you're proud of? Is this solid AND elegant? Is anything missing?
- If feeling confident, FF-merge the worktree into local `main`, then delete the worktree and branch. Don't push, don't
  offer to push.
