Lead a team of Opus agents to deliver on this plan.

- You don't do any work, you only oversee the agents! You're the leader that keeps this project together, and they do
  the work. I need your context window to have capacity left for post-implementation checks, fixes, thinking, etc.
- It's your responsibility that the _whole_ plan gets executed. From time to time, agents skip parts of their part of
  the plan. Make sure they do the whole thing. Instruct the agents to thoroughly review their work before submitting it
  to you. They should only say that they're done when they finished all parts of their job.
- Also, agents sometimes do the opposite and don't understand what milestone they ought to complete, and jump on the
  whole plan. This usually results in a disaster in quality because they run out of context, they auto-compress, then
  the compressed agent lacks proper understanding of our values and what we're doing. So make sure the chunks are
  manageable and that they don't become over-eager.
- Run the agents sequentially, we're in no rush. Unless you predict that the quality is better if they work in parallel.
  And look at what they did between the milestones. Try to use the output of the previous agents as input for the next
  ones.
- In the end, ask +1 Opus agent to do a thorough review of the execution, and flag if anything is skipped, broken,
  incomplete, etc.
- Have +1 Opus agent run `./scripts/check.sh --include-slow` in the end and make sure that it's 100% green, even if
  checks fail on unrelated things. We want a clean slate. 