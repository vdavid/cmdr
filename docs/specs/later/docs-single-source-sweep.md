# Docs single-source reconciliation sweep

**Goal:** find every place where a load-bearing technical mechanism is described in more than one doc, designate a
single canonical home for each, and reduce the other copies to pointers. Enforces the `docs-single-source` rule
(`.claude/rules/docs-single-source.md`) across the existing docs, where today it's only honored ad hoc.

**Why:** copied prose rots independently. The worked example: the macOS Full Disk Access detect-vs-register mechanism
was described in `permissions.rs` comments, `onboarding/DETAILS.md`, and `architecture.md` simultaneously; when the real
Tahoe behavior turned out to be the opposite of what was written, it was wrong in all three and had to be fixed three
times. `architecture.md` is the highest-risk accreting doc because it's the map everyone reads first, so it tends to
grow "how" instead of staying "what + where + pointer."

This is a deferred cleanup, not blocking anything. Run it as a single multi-agent sweep when there's appetite.

## Scope

- **Phase 1 (primary, do this first): `docs/architecture.md` vs its colocated docs.** Every bullet that describes a
  _mechanism_ (a flow, a trigger, a syscall sequence, a decision and its rationale) instead of map-info (what a
  subsystem is, where it lives, a one-line pointer) is a candidate. The canonical home is the colocated
  `DETAILS.md`/`CLAUDE.md` or the module doc named by that bullet.
- **Phase 2 (optional, broader): `CLAUDE.md` <-> `DETAILS.md` duplication within a feature dir,** and the same mechanism
  restated across sibling `DETAILS.md` files. Lower yield, higher effort; gate on whether Phase 1 surfaced enough to
  justify it.

Out of scope: code comments themselves (handled per-file as code changes), UI copy, the changelog, and anything under
`brand/` or human-facing site copy.

## Orchestration: hub Opus agent + auditor swarm

Run with the `Workflow` tool (or a hub agent spawning subagents). Three phases, pipelined where possible.

### Phase 1 - partition + audit (fan-out, read-only)

- The hub splits `architecture.md` into its natural sections (Frontend, Backend, Other apps, Search, Cross-cutting
  platform constraints, macOS specifics, Dev mode, Diagnostics, Tooling). One **auditor subagent per section** (~8-10
  agents), each read-only.
- Each auditor, for every bullet in its section:
  1. Classify: **map-shape** (what + where + pointer; leave alone) or **mechanism-describing** (candidate).
  2. For each candidate, open the colocated `DETAILS.md`/`CLAUDE.md`/module doc the bullet refers to and decide:
     - **trim-to-pointer**: the mechanism already lives in the canonical home -> propose replacing the architecture.md
       prose with a one-line "what + where + see `<path>`".
     - **move-then-point**: the mechanism lives _only_ in architecture.md -> propose moving the detail down into the
       canonical home first, then pointing. Never delete detail that has no other home.
  3. Flag any **evidence-anchorless volatile claim** (OS/version/empirical) it passes, so the apply step can tag it.
- Each auditor returns a structured list:
  `{ bullet, classification, canonical_home_path, action, proposed_pointer_text, proposed_move_text?, notes }`. Use a
  JSON schema so the hub gets validated objects, not prose.

### Phase 2 - reconcile + apply (hub, single writer)

- The hub dedupes/merges proposals, resolves cross-section overlaps (a mechanism referenced from two sections gets one
  home), and **applies all edits itself** (single writer, no parallel file mutation). Order: do every `move-then-point`
  (write the canonical home) before the matching `trim-to-pointer`, so no detail is ever momentarily homeless.
- Apply the evidence-anchor tags surfaced in Phase 1.

### Phase 3 - verify + land

- Run `pnpm check oxfmt docs-reachable` (markdown formatting + the orphan-doc check). `docs-reachable` is an error-level
  check, so any doc the sweep newly points at must stay reachable.
- Spot-check that no bullet lost information (diff each `architecture.md` removal against the canonical home it moved
  to).
- Commit on a worktree branch, rebase onto local `main`, FF-merge. One commit, impact-led message.

## Guardrails

- **Never lose information.** Move-then-point, never delete. If a mechanism has no canonical home, create one (a missing
  `DETAILS.md`) rather than dropping the detail.
- **Preserve evidence anchors** (`(verified on X, by Y)`) verbatim when moving them; add them where a volatile claim
  lacks one.
- **Read-only auditors, single-writer hub.** Auditors must not edit; only the hub writes, to avoid parallel-edit
  conflicts on shared files.
- **Don't touch code** (`.rs`/`.ts`/`.svelte`) or generated files. Docs only. If a code comment duplicates a doc, note
  it for a follow-up, don't edit it here.
- Follow `docs-single-source.md`, `docs-maintenance.md` (describe current, not history), and the `agent-facing-docs`
  user rule (structure for retrieval, not visual scannability).

## Deliverables

- A reshaped `architecture.md` that is map-only (what + where + pointer), with mechanism detail living in canonical
  homes.
- Any canonical homes created/extended to receive moved detail.
- A short summary in the commit body of what moved where, and a count of duplications collapsed.

## Sizing

Phase 1 is ~8-10 read-only auditor agents over one ~250-line file plus the docs they reference; Phase 2-3 is one writer.
Rough order: a few hundred K tokens for a thorough pass. Cheap relative to the rot it prevents.
