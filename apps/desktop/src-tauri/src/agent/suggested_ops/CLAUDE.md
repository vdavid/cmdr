# Suggested ops (`agent/suggested_ops/`)

The service over the proposal spine: turning a selector into a frozen op list, and reporting acceptance rate. The rows,
the lifecycle machine, and the claim transaction are `../store/proposals/CLAUDE.md`'s. Depth: `DETAILS.md`.

## Module map

- `mod.rs` — `propose` / `approve` / `started` / `reject` (thin over the store, plus the metric) and selectors.
- `bridge/` — a file group's approval: the claim, the executor call, and the decorator writing outcomes back.
- `server/` — a server card's approval (`DETAILS.md` § Approving a server).
- `selector.rs` — `OpSelector`, the `SelectorIndex` seam, and `DriveIndex`, the drive-index resolver.
- `analytics.rs` — the three analytics events. `changed.rs` — the pending set moved. `facts.rs` — the review's facts.

## Must-knows

- **We do not trust the agent.** A suggestion can be formally valid and factually hallucinated, and we can never know
  which, so the job is to lay it all out for the user to decide. ❌ Never add an agent-specific safety behaviour to the
  execution path (no auto-skip on collision, no refusing an overwrite or an irreversible group): once the user
  approves, it is as if they started the action (`docs/design-principles.md`). Put the effort into disclosure.
- **A selector resolves ONCE, at creation, against the drive INDEX**, never at approval (a test counts the calls):
  that's what makes "what the user saw is what runs" true. ❌ Never by walking the filesystem: a walk blocks on a dead
  mount and reads ground the user never consented to index.
- **The matcher is `search::matcher::CompiledQuery`.** ❌ Never re-derive glob translation or case folding here: that
  fork is how a selector and the search box would disagree about the same `*.dmg`.
- **A refusal is not an empty list.** `SelectorRefusal::NotIndexed` says "I can't see that drive", a different answer
  from "nothing matched". Branch on the variant.
- **Analytics carry a verb and a bucketed count**, ❌ never a path, file name, rationale, or selector pattern: those
  are the user's own data, and they stay local.
- **The bridge builds an ORDINARY executor call**, through the same routed entry points a click uses, with the default
  config. Its bookkeeping never reaches the filesystem.
- **Reporting flows engine → store, never the reverse.** The decorator wraps the injected sink and writes
  `proposal_ops.status` from the per-source outcomes; the engine's crate can't name `agent::`.
- **The agent hears every answer** (`../outcomes.rs`). A rejection fires in `reject`'s `Rejected` arm, which the
  conditional UPDATE makes once-per-group across restarts; an APPROVAL fires at SETTLE, in the decorator, because
  `approve` is only a claim and a claimed group can still skip every file. ❌ Never move that one, or the approval
  METRIC (`started`, since a refused start gives the group back), to the claim. The thread's "running" line is written
  at the bridge's START and rewritten in place at settle (`DETAILS.md` § What the user's answer teaches the agent).
- **A dismissed dialog is not a rejection.** `cancel_bulk_rename_proposal` passes `RejectSource::DialogDismissed`, so
  nothing is learned and no turn is asked for: that sweep's `conversation_id` is the user's active RAIL thread.
- **The execution binding is a LIVE stat at preflight**, ❌ never the stored snapshot: the two answer different
  questions and only one is about a race (`DETAILS.md`). Fingerprints stay in memory, so a restart re-preflights.
- **Every path that moves the pending set announces it** (`changed.rs`), so surfaces subscribe rather than poll. An
  amend emits no ANALYTICS (one decision, counted once) but still announces.
- **Symlinks are skipped during resolution**: their indexed size and date describe the LINK, not the file.

The review dialog: `apps/desktop/src/lib/suggested-ops/CLAUDE.md`. What wakes the agent: `../wake/CLAUDE.md`.
