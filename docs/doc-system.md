# Cmdr's doc system

How Cmdr's agent-facing docs are structured and maintained. The resident contract is `AGENTS.md` § Docs (it loads in
every session); this is the depth behind it: the model, the rationale, David's principles, and the playbook for a
slimming pass. Read this before any broad doc cleanup, or before slimming a `CLAUDE.md` / `DETAILS.md` with David.

## The three tiers

Docs load in three concentric rings, by how often an agent needs them:

- **Resident (Ring 0)**: the repo-root `CLAUDE.md` (which `@`-imports only `AGENTS.md`) plus `.claude/rules/*.md`. Loads
  in EVERY session, worktree, and subagent: this is the whole always-paid budget, so it stays tiny. It holds what Cmdr
  is, the 4-app map, the router, the universal rails, the doc contract, and the writing-voice core. `architecture.md`,
  `style-guide.md`, and `design-principles.md` are deliberately NOT resident; they're read on demand via the router.
- **Path-scoped (colocated `CLAUDE.md`)**: auto-injected when an agent touches a directory. Must-knows only. Area rules
  live here too (Rust + Tauri rules in `apps/desktop/src-tauri/CLAUDE.md`, frontend rules in
  `apps/desktop/src/CLAUDE.md`, testing rules in `apps/desktop/test/CLAUDE.md`), so an agent working on another app
  never loads them.
- **On-demand (`DETAILS.md`, `docs/guides/`, the `architecture.md` map)**: read when the task calls for it, found via
  the router, colocated pointers, and CodeGraph. Unlimited length.

## C vs D: the litmus

Every line in a `CLAUDE.md` must pass: **"would an agent editing a file in this directory get something wrong, or
silently break something, if it never saw this line?"** If not, it's not a must-know: it belongs in `DETAILS.md` or gets
cut. Keep in `CLAUDE.md`: invariants, gotchas, "don't do X because Y" guardrails, a 2-3 line module map, and the
`DETAILS.md` pointer. Everything else (architecture narrative, data flows, decision rationale, history, edge-case
catalogs, benchmarks) is depth.

- **No word floor.** A `CLAUDE.md` should be as small as its essentials allow. If 80 words captures the must-knows, 80
  is right; don't pad toward the 600-word ceiling. Dense and focused beats comprehensive.
- **Condense before you move.** Most bloat is padded wording, not misplaced depth: a typical `CLAUDE.md` halves with
  zero information loss before anything moves to `DETAILS.md`. Tighten the prose first, then move what's genuinely
  depth.
- **Don't pad `DETAILS.md`.** Move only real depth. Every area has a `DETAILS.md` by mandate, and a near-stub one is
  correct when the area has little to say. Never invent content to fill it.

## Running a slimming pass

1. **Read the code, not just the doc.** Verify each must-know against the actual source before keeping it; fix or drop
   stale claims. The checks guarantee structure, never truth, and past passes found real drift (wrong command counts, a
   wrong MTP path format, wrong API names, a stale license grace period).
2. **Apply the litmus; condense first, move depth second.** Get each `CLAUDE.md` to dense essentials.
3. **Follow the principles below.**
4. **Run the checks** (below) and read the output in full (don't pipe `pnpm check` through `head` / `tail`).

For a large pass (dozens of files), a subagent swarm works well: partition disjoint directories across agents (each owns
one `CLAUDE.md` plus its sibling `DETAILS.md`), hand them this litmus and these principles, forbid `git` and the writing
checks (they mutate shared allowlists mid-run), and review every diff yourself. See `guides/multi-agent-refactors.md`.

## Principles (David's, for all docs)

- **Mandatory sibling `DETAILS.md`.** Every area `CLAUDE.md` has a `DETAILS.md` in its directory and links it, so the
  "should this area have a `DETAILS.md`?" decision never recurs: it always does. Default new content to `DETAILS.md`;
  promote to `CLAUDE.md` only past the must-know bar.
- **Canonical closing-pointer line.** Every area `CLAUDE.md` ends with a one-line pointer to its `DETAILS.md` that names
  the payload AND carries a read-trigger biased toward over-reading. The trigger sentence is fixed:
  `Read it before any non-trivial work here: editing, planning, reorganizing, or advising.` So the line reads
  `<what's inside>: [DETAILS.md](DETAILS.md). <trigger>` (default the payload to "Architecture, flows, and decisions"
  when nothing more specific fits). The low bar ("any non-trivial work") plus the named activities are deliberate: the
  trigger calls out planning/reorganizing/advising, not just editing, because the old "before structural changes"
  wording let reorg and advice tasks skip the doc. `DETAILS.md` headers mirror it with "Read this before ...".
- **Describe current state, not history** (`.claude/rules/docs.md`): git holds the history. Drop "we originally / used
  to / no longer applicable" narration; keep the non-obvious why and constraint-encoding pain.
- **Single-source**: a mechanism lives in ONE canonical doc; everywhere else points to it by path. `architecture.md` is
  a map (what + where + a pointer), never how.
- **Evidence-anchor volatile claims**: OS, version, and empirical claims carry `(verified on <env>, <method>, <date>)`.
- **Agent-facing style** (`style-guide.md` § Agent-facing docs): no two-column tables and no column wider than 100 chars
  (bullet lists instead), sentence case, en-dash not em-dash, Oxford comma. Docs are a token stream, not a 2D layout.
- **Never bump an allowlist without David's OK** (`.claude/rules/file-length-allowlist.md`): trim or split instead;
  leaving a warn is safe.

## Enforcement (the checks that keep it honest)

Convention rots; checks don't. Each invariant is a check (sources in `scripts/check/checks/`, detail in its
`scripts/check/checks/DETAILS.md`):

- **`resident-doc-budget`** (warn): caps the always-resident bundle (root `CLAUDE.md` + its `@`-imports +
  `.claude/rules/`); the cap ratchets DOWN only. Guards against silent regrowth of the per-session cost.
- **`claude-md-length`** (warn): caps each `CLAUDE.md` at 600 words; shrink-wraps its allowlist.
- **`claude-md-details-sibling`** (error): every non-root `CLAUDE.md` has a sibling `DETAILS.md` and links it.
- **`docs-reachable`** (error): every doc reachable from the root `CLAUDE.md` by link-walking (no orphans).
- **`docs-dead-links`** (error): no relative link points at a missing file.

## Why this exists (the constraint to defend)

The always-resident bundle was once ~9,500 words, and it's paid again on every worktree and subagent, so a cross-cutting
task across a worktree boundary burned tens of thousands of tokens in auto-injected docs before any real work. The diet
cut it to ~2,000 and made the smallness enforced rather than hoped-for. The keystone is `AGENTS.md` § Docs: keep it
crisp and resident, because it's the contract every future agent reads and replicates. If it drifts or bloats, the whole
system drifts with it.
