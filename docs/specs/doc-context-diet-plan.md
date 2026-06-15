# Doc context diet plan

Shrink what every agent loads unconditionally, and make the smallness self-enforcing. The lever is not "shorter docs"
for its own sake: it is moving content from the resident tier (paid in every session, worktree, and subagent) into
path-scoped and on-demand tiers, and encoding the discipline as checks so it cannot regrow.

## Why (measured 2026-06-15)

Resident on every fresh session, before a single file is read:

- `AGENTS.md` 3,045 words, `docs/architecture.md` 2,738, `docs/style-guide.md` 1,533, `docs/design-principles.md` 440,
  project `.claude/rules/*` (12 files) 1,710. Project resident subtotal: ~9,466 words.
- User-level (`~/.claude/CLAUDE.md` 1,660 + `~/.claude/rules/*` 2,535): ~4,195 words. Out of scope for this repo but
  noted: it is paid in every project.
- Total resident: ~13,660 words ≈ ~18k tokens.

The leaks:

- **Most of the root docs are desktop-scoped.** `architecture.md` is ~74% pure desktop (Frontend 735 + Backend 700 +
  Search 86 + Diagnostics 105 + most cross-cutting 466). `AGENTS.md` is ~29% desktop (Running, Debugging, Testing-via-
  MCP, worktree mechanics, half of File structure). A website or api-server agent carries ~3,000 words of desktop
  internals for nothing.
- **The colocated `CLAUDE.md` budget is fiction.** The `claude-md-length` check declares a 600-word budget, yet all 65
  allowlisted entries exceed it (34 over 1,000, 13 over 1,500, 7 over 2,000; allowlisted sum 75,363 words). The
  allowlist ratcheted up to current sizes and froze, certifying the bloat.
- **This multiplies on the real workflow.** Discovery reads colocated docs; the fresh worktree re-injects them when the
  same areas are re-touched; subagents pay the resident bundle again. A cross-cutting task across a worktree boundary
  burns tens of thousands of tokens in auto-injected docs before any real reading. Every word trimmed from a hot-path
  colocated file is saved 2x+.

## The model: three tiers

- **Resident (every agent, every worktree, every subagent).** Only what passes one test: "would an agent doing *any*
  task get something wrong, or violate something, if it never saw this?" Almost nothing currently resident passes.
  Target: what-is-Cmdr, the 4-app map, the universal rails, and a router. ~3,000–3,500 project words.
- **Path-scoped auto (colocated `CLAUDE.md`).** Loads when an agent works in that area. Must-knows only, ≤600 words.
  This is the one reliable conditional-load mechanism, so it carries the weight.
- **On-demand (`DETAILS.md`, guides, the architecture map, deep style-guide sections).** Read when the task calls for
  it, found via the router + colocated pointers + CodeGraph. Unlimited, never auto-injected.

CodeGraph note: the architecture map's "where does X live" content duplicates what `codegraph_search` answers live and
fresh. The map keeps existing as an on-demand doc (CodeGraph is absent in web sessions and fresh clones), but it stops
being resident.

## Decisions locked in this plan

- **Every area `CLAUDE.md` must have a sibling `DETAILS.md`, and link it.** Repo-root `CLAUDE.md` is the only exception.
  Mandate it as a stub (`# <area> — details` + one line: "Depth and rationale live here; CLAUDE.md holds only
  must-knows.") so the C→D link has a real target and the create-decision disappears. Rationale: the *existence
  decision* is the friction that makes agents cram depth into C; removing it leaves only the right question (must-know
  vs depth). Enforced by a check (presence + link), folded into the `claude-md-length` walker.
- **All `CLAUDE.md` ratchet to ≤600 words**, allowlist trending to empty. Migration is per-file and careful (dropping a
  must-know is a real regression), not a mechanical truncation. Do the 13 files over 1,500 first. Keep the check at warn
  during migration; David watches growth.
- **Most rules relocate.** Only universal rails stay resident; area rails move to the colocated `CLAUDE.md` they govern;
  check-only rules become doc-pointers. See Phase 3.
- **The keystone is `AGENTS.md § Docs`:** the crisp, resident contract for how this doc system works (the C/D split, the
  tier test, the 600 rule, the mandatory-D rule, the router). The unenforceable risks (content in the wrong tier, rot)
  collapse onto this doc being clear and resident. Keep it tight; it is what future agents replicate.

## Phases

Ordered by leverage. Phase 0 first (makes the rest stick), then a slice of Phase 4 to prove the numbers move.

### Phase 0 — Durability checks (do first)

- Add a **resident-bundle check**: sum root `CLAUDE.md` transitive `@`-imports + project `.claude/rules/*`, warn past a
  cap. Start the cap at the current number and ratchet down. Same shrink-wrap allowlist pattern as
  `file-length-allowlist`. Put it in the `claude-md-length` check family (same Go check family, same allowlist style) —
  unless review prefers a standalone check.
- Extend the `claude-md-length` walker to enforce **sibling `DETAILS.md` presence + a C→D link** for every area
  `CLAUDE.md`.

### Phase 1 — Re-home desktop content out of monorepo root (biggest goal-3 win)

- Move `AGENTS.md` Running / Debugging / Testing-via-MCP / desktop worktree mechanics into `apps/desktop/CLAUDE.md`
  (must-knows) + `apps/desktop/DETAILS.md` (procedural depth; point to existing `docs/tooling/logging.md` etc., do not
  copy).
- Move `architecture.md` Frontend + Backend + Search + Diagnostics subsystem maps into `apps/desktop/DETAILS.md`. The
  monorepo `architecture.md` keeps only: the 4-app map, cross-app concerns (acquisition analytics, external services),
  and a pointer to each app's own map.
- Move cross-cutting platform constraints (Tauri IPC threading, network-mount syscalls, two-layer timeouts) into
  `apps/desktop/src-tauri/CLAUDE.md` (or `DETAILS.md`), scoped to the code they guard.

### Phase 2 — De-`@`-import and build the router

- Root `CLAUDE.md` `@`-imports almost nothing; it inlines a lean resident block + a task→location router. Draft:

  ```
  # Cmdr — monorepo root
  Cmdr is a fast, keyboard-first two-pane file manager (Rust + Tauri 2 + Svelte 5), macOS-only, open beta.
  4 apps: desktop (the app), website (getcmdr.com), api-server (CF Worker), analytics-dashboard. Each has its CLAUDE.md.

  ## Where to look (router)
  - Editing code → the colocated CLAUDE.md loads automatically. "Where does X live" → CodeGraph, not a doc.
  - Planning an unfamiliar area → docs/architecture.md (the map).
  - A procedure (release, screenshots, deps) → docs/guides/ + the skills.
  - Branding/marketing → brand/CLAUDE.md, apps/website, README. Skip app internals.

  ## Universal rails
  - Worktrees, no PRs, no push without ask, FF-merge to local main.
  - Run `pnpm check` at the right cadence; finish every unit of work with it.
  - Voice: active, friendly, sentence case, no em-dash, Oxford comma, ISO dates. Full: docs/style-guide.md.
  - TDD red→green where reasonable. Describe current state, not history.

  ## Principles (full: docs/design-principles.md)
  Delightful UX · elegance over hacks · rock-solid responsiveness · protect user data · respect resources ·
  humans-to-humans for anything human-facing.
  ```

- `architecture.md` and `design-principles.md` become read-on-demand (de-`@`-imported).

### Phase 3 — Rules diet

- **Stay resident (universal):** `git-conventions` (trimmed; much overlaps user-level commit rules), a merged
  docs-discipline rule (`docs-single-source` + `docs-maintenance`), `no-ignored-warnings`. ~400–500 words total.
- **Relocate to colocated:** `rust` → `src-tauri/src/CLAUDE.md`, `tauri-apis` → desktop, `frontend` → `src/CLAUDE.md`,
  `dev-security` → desktop. ~100 words each against that file's budget; loads path-scoped.
- **Demote to doc-pointer:** `file-length-allowlist` → a doc the check points to.
- **Open call:** `no-string-matching` spans Rust and TS. Colocating duplicates it; it is linter-enforced anyway. Lean
  toward keeping it as one small resident rule. Confirm with David.
- Caveat (accepted): a relocated rail moves from "always on" to "on when in-area". An agent creating the first file in
  an area without reading a sibling could miss it; in practice the area `CLAUDE.md` injects on touching anything under
  it. Negligible gap, constant waste removed.

### Phase 4 — Ratchet colocated `CLAUDE.md` to ≤600 (bulk of the effort, most savings)

- Per-file: move depth into the (now-mandatory) `DETAILS.md`, keep only "edit here and silently break X without this".
- Order: the 13 files over 1,500 first (network 2,421, listing 2,395, analytics-dashboard 2,381, commands lib 2,355,
  commands tauri 2,229, downloads lib 2,165, volumes 2,101, and the rest over 1,500), verify no must-know lost, then the
  remainder.
- Best run as a multi-agent refactor (see `docs/guides/multi-agent-refactors.md`) with per-file verification, not a
  mechanical pass.
- When done, empty the allowlist; the check self-enforces 600.

## Self-maintenance: what guarantees the status quo holds

Enforceable invariants become checks (self-maintaining): resident-bundle cap, C ≤600, sibling-D-present-and-linked,
`docs-reachable`. Unenforceable ones (right-tier content, freshness) lean on the keystone contract doc plus David's
review and the `describe-current-not-history` discipline. Both non-negotiables: encode every enforceable invariant as a
check, and keep `AGENTS.md § Docs` crisp and resident.

## Open decisions to confirm before executing

- Resident-bundle check: fold into `claude-md-length` family, or standalone? (Plan assumes: fold in.)
- `no-string-matching`: keep resident, or duplicate into the two trees? (Plan assumes: keep resident.)
- `DETAILS.md` stub mass-creation across ~227 dirs: script it in one pass, then the check holds the line. (Plan assumes:
  yes, scripted.)
