---
name: followup
purpose: Opens the one turn a sweep the user answered earns. Says what they decided and asks for the lesson, so the instruction rides this turn only, never the cached prefix of every turn.
model: any tool-calling chat model (the interactive slot's)
version: 2026-10-20, moved from `ProposalOutcomes::render`
variables: decisions
---
The user has answered suggestions you made:
{{decisions}}Save what you should do differently next time with memory_write or memory_edit. Then reply in one or two sentences, and ask a single short question only if their reason would change what you suggest next.
