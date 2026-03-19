Lead a team of Opus agents to deliver on this plan.

You:
- You don't do any work, only oversee the agents!
- You're the leader that keeps this project together, and they do the work. I need your context window to have capacity
  left for post-implementation checks, fixes, thinking, etc.
- Run the agents sequentially, we're in no rush. Unless you predict that the quality is better if they work in parallel.
  And look at what they did between the milestones. Try to use the output of the previous agents as input for the next
  ones.

Agents:
- It's your responsibility that the _whole_ plan gets executed. From time to time, agents skip parts of their part of
  the plan. Give them a clear scope and ask them to do the whole thing. Instruct the agents to thoroughly review their
  work before submitting it to you. They should only say that they're done when they finished all parts of their job.
- Also, agents sometimes do the opposite and don't understand what milestone they ought to complete, and jump on the
  whole plan. This usually results in a disaster in quality because they run out of context, they auto-compress, then
  the compressed agent lacks proper understanding of our values and what we're doing. So again, give them a clear scope.
- Ask every agent to reflect whether they are satisfied with what they'd done. Make them ask: "Is what I've done solid
  AND elegant? You proud and confident about it?" — If the answer is "no" to either, they should adjust, then rinse and
  repeat.
- They should also see if there is something else to fix, like any latent bugs that only need 10–15 LoC changes around
  their part of the development. They're encouraged to fix these. Correctness and bug-free code over crystal-clean
  commits.

Final review:
- In the end, ask +1 Opus agent to do a thorough review of the execution, and flag if anything is skipped, broken,
  incomplete, etc.
- Have +1 Opus agent run `./scripts/check.sh` and make sure that it's green, even if checks fail on unrelated things.
- Do a review yourself, and report: is this something you're proud of? Is this solid AND elegant? Is something missing?