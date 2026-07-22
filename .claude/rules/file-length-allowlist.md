# Allowlist consent

The warn-only scanners keep JSON allowlists of current sizes: `file-length` (file line counts) and `claude-md-length`
(CLAUDE.md word counts), plus the error-level `docs-reachable` (intentionally-unreachable docs). They shrink-wrap
themselves on local runs (drop gone/satisfied entries, ratchet >10% slack down), so don't hand-edit the `files`
sections: run `pnpm check file-length` (or the relevant check) and commit the rewrite.

❌ Never add a new entry, raise an existing number, or otherwise loosen a contract without explicit user consent. The
allowlist tracks current sizes; bumping it as a side effect of a change hides growth that should be fixed by trimming or
splitting (for a `CLAUDE.md`, by moving depth into its `DETAILS.md`). The length checks are warn-only, so leaving a warn
is always safe: surface it to David rather than silencing it. `docs-reachable` is an error, so connect an orphan rather
than exempt it.

Per-allowlist mechanics and the `exempt` section (generated files like `bindings.ts`):
`scripts/check/checks/DETAILS.md`.
