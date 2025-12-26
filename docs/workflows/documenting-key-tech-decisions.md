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

## Context

What is the issue we're facing? What constraints exist?

## Decision

What are we doing about it?

## Consequences

### Positive

- What becomes easier/better

### Negative

- What becomes harder/worse

### Notes (optional)

- Additional context, links, future considerations
```

3. Link from `AGENTS.md` (or code comments if relevant)
4. Do not commit unless asked to

## Examples of decisions worth documenting

- Choosing between competing libraries/frameworks
- Changing build/test processes
- Disabling/enabling linter rules project-wide
- Adopting new conventions that differ from defaults
