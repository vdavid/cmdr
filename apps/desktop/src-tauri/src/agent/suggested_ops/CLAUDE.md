# Suggested ops (`agent/suggested_ops/`)

The service over the proposal spine: turning a selector into a frozen op list, and reporting acceptance rate. The rows,
the lifecycle machine, and the claim transaction are `../store/proposals/CLAUDE.md`'s. Depth: `DETAILS.md`.

## Module map

- `mod.rs` — `propose` / `approve` / `reject` (thin over the store, plus the metric) and `resolve_selector_ops`.
- `bridge/` — approval to running operation: the claim, the executor call, and the sink decorator that writes
  outcomes back.
- `selector.rs` — `OpSelector`, the `SelectorIndex` seam, and `DriveIndex`, the drive-index resolver.
- `analytics.rs` — the three PostHog events. `changed.rs` — the one signal that the pending set moved.

## Must-knows

- **We do not trust the agent.** A suggestion can be formally valid and factually hallucinated, and we can never know
  which, so the job is to lay it all out for the user to decide. ❌ Never add an agent-specific safety behaviour to the
  execution path (no auto-skip on collision, no refusing an overwrite or an irreversible group): once the user
  approves, it is as if they started the action. Put the effort into disclosure.
- **A selector resolves ONCE, at creation, against the drive INDEX.** Never at approval, which needs no rule:
  `tests.rs::a_selector_freezes_at_creation_and_is_never_resolved_again` counts the calls. Freezing is what makes "what
  the user saw is what runs" true. ❌ Never resolve by walking the filesystem, though: nothing catches that, and a walk
  blocks on a dead mount and reads ground the user never consented to index.
- **The matcher is `search::matcher::CompiledQuery`.** ❌ Never re-derive glob translation or case folding here: that
  fork is how a selector and the search box would disagree about the same `*.dmg`.
- **A refusal is not an empty list.** `SelectorRefusal::NotIndexed` says "I can't see that drive", a different answer
  from "nothing matched". Branch on the variant, as `error-string-match` already requires.
- **Analytics carry a verb and a bucketed count, never a path**, a file name, a rationale, or a selector pattern. Those
  are the user's own data, and `main.db` is a map of their life that stays local.
- **The bridge builds an ORDINARY executor call**, through the same routed entry points a click uses, with the default
  config, which is where the rule above stops being a slogan. Its bookkeeping never reaches the filesystem.
- **Reporting flows engine → store, never the reverse.** The decorator wraps the injected sink and writes
  `proposal_ops.status` from the per-source outcomes; `write-ops-isolation` fails the build if `write_operations` ever
  names `agent::`.
- **The agent hears every answer** (`../outcomes.rs`). A rejection fires in `reject`'s `Rejected` arm, which the
  conditional UPDATE makes once-per-group across restarts; an APPROVAL fires at SETTLE, in the decorator, because
  `approve` is only a claim and a claimed group can still skip every file. ❌ Never move that one to the claim.
- **A dismissed dialog is not a rejection.** `cancel_bulk_rename_proposal` passes `RejectSource::DialogDismissed`, so
  nothing is learned and no turn is asked for: that sweep's `conversation_id` is the user's active RAIL thread.
- **The execution binding is a LIVE stat at preflight**, ❌ never the stored snapshot: the two answer different
  questions and only one is about a race (`DETAILS.md`). Fingerprints stay in memory, so a restart re-preflights.
- **Every path that moves the pending set announces it** (`changed.rs`), so surfaces subscribe rather than poll. An
  amend emits no ANALYTICS (one decision, counted once) but still announces.
- **Symlinks are skipped during resolution.** The index doesn't follow them, so their size and date describe the LINK,
  not the file the user is deciding on.

The review dialog: `apps/desktop/src/lib/suggested-ops/CLAUDE.md`. What wakes the agent: `../wake/CLAUDE.md`.
