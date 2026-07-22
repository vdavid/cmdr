# specs/ details

Read this before reorganizing the specs folder or its lifecycle conventions.

- **What lives here**: per-development specs and task lists (plans), indexed in `index.md`. Not a description of
  codebase state; temporary working docs kept for reference, like ADRs.
- **Wipe policy**: this folder gets wiped periodically once each shipped plan's durable intent (feature rationale,
  process) is captured in code or colocated `CLAUDE.md` / `DETAILS.md`. Full statement: `README.md`.
- **`later/`**: deferred work that survives a wipe. Same index discipline; see `later/`.
- **Discipline**: update `index.md` whenever you add or modify a plan, so each stays discoverable.
