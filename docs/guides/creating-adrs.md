---
description: How to document important technical decisions
---

When making a significant technical decision (tech choice, architecture pattern, process change):

1. Create an ADR in `docs/adr/NNN-decision-name.md` (use next available number)
2. Use this template:

```markdown
# ADR NNN: [Decision Title]

## Status

Accepted | Proposed | Deprecated | Superseded by ADR-XXX

## Summary

A short summary of the context, problem, solution, and consequences. In clear language, 3–4 sentences.

## Context, problem, solution

### Context

Give all information that helps understand the background and reasoning behind the decision, everything that leads up to
the problem statement, but not the problem itself.

### Problem

What is the issue we're facing? What constraints exist? How big is the impact? What are the non-goals?

### Possible solutions considered

What approaches have been considered to solve the problem, but not chosen? What were their tradeoffs?

### Solution

What did we decide to do? Why did we pick this? Elaborate on what we'll do.

## Consequences

### Positive

What becomes easier/better

### Negative

What becomes harder/worse

### Notes (optional)

Additional context, links, future considerations
```

3. Link in `AGENTS.md` (or in code comments if relevant)

## Examples of decisions worth documenting

- Technological choices: languages, libraries,frameworks, build/test processes, tooling
- Architectural decisions
- Adopting conventions that differ from defaults
