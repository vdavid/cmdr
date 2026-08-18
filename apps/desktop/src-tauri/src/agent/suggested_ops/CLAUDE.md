# Suggested ops (`agent/suggested_ops/`)

The service over the proposal spine: turning a selector into a frozen op list, and reporting acceptance rate. The rows,
the lifecycle machine, and the claim transaction belong to `../store/proposals/CLAUDE.md`; this layer is everything
above them that a row can't hold. Depth: `DETAILS.md`.

## Module map

- `mod.rs` — `propose` / `approve` / `reject` (thin over the store, plus the metric) and `resolve_selector_ops`.
- `selector.rs` — `OpSelector`, the `SelectorIndex` seam, and `DriveIndex`, the drive-index resolver.
- `analytics.rs` — the three PostHog events.

## Must-knows

- **We do not trust the agent.** A suggestion can be formally valid and factually hallucinated, and we can never know
  which, so the job is to lay everything out for the user to decide. ❌ Never add an agent-specific safety behaviour to
  the execution path (no auto-skip on collision, no refusing an overwrite, no refusing an irreversible group): once the
  user approves, it is exactly as if they started the action. Put the effort into disclosure instead.
- **A selector resolves ONCE, at creation, against the drive INDEX.** ❌ Never at approval, and never by walking the
  filesystem (a walk blocks on a dead mount and reads ground the user never consented to index). Freezing is what makes
  "what the user saw is what runs" true; a test pins that the resolver is asked exactly once.
- **The matcher is `search::matcher::CompiledQuery`.** ❌ Never re-derive glob translation or case folding here: that
  fork is how a selector and the search box would disagree about the same `*.dmg`.
- **A refusal is not an empty list.** `SelectorRefusal::NotIndexed` says "I can't see that drive", which is a different
  answer from "nothing matched". Branch on the variant, as `error-string-match` already requires.
- **Analytics carry a verb and a bucketed count, never a path**, a file name, a rationale, or a selector pattern. All of
  those are the user's own data, and `main.db` is a map of their life that stays local.
- **Symlinks are skipped during resolution.** The index doesn't follow them, so their size and date describe the LINK,
  and a proposal built on those would show facts about something other than the file the user is deciding on.

The feature plan (verbs, milestones, the review dialog): `docs/specs/agent-suggested-ops-plan.md`.
