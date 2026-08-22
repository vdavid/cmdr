# Allowlist consent

The warn-only scanners keep JSON allowlists of current sizes: `file-length` (file line counts), `claude-md-length`
(CLAUDE.md word counts), `invariant-density` (`❌` rules per subsystem), `module-cycles` (module tangle sizes per home),
and `jscpd-rust` / `jscpd-frontend` (duplicated lines per file pair), plus the error-level `docs-reachable`
(intentionally-unreachable docs). They shrink-wrap themselves on local runs (drop gone/satisfied entries, ratchet slack
down), so don't hand-edit the `files` / `subsystems` / `pairs` / `tangles` sections: run `pnpm check file-length` (or
the relevant check) and commit the rewrite.

❌ Never add a new entry, raise an existing number, or otherwise loosen a contract without explicit user consent. The
allowlist tracks current sizes; bumping it as a side effect of a change hides growth that should be fixed by trimming or
splitting (for a `CLAUDE.md`, by moving depth into its `DETAILS.md`; for the jscpd lanes, by extracting the shared
code). These checks are warn-only, so leaving a warn is always safe: surface it to David rather than silencing it.
`docs-reachable` is an error, so connect an orphan rather than exempt it.

**`invariant-density` is exempt from that rule**: add entries and raise numbers freely, no need to ask. It counts `❌`
rules per subsystem, and a rule earns its place on whether the invariant is worth stating, which the number can't judge.
Encoding an invariant in a type is still better than writing a rule about it; the count just isn't the thing that
decides.

Per-allowlist mechanics and the `exempt` section (generated files like `bindings.ts`):
`scripts/check/checks/DETAILS.md`.
