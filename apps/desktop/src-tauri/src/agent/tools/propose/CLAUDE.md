# Rename proposals

`rename/` stages Ask Cmdr's immutable, in-memory rename plans. It validates only cached pane and index state: never probe a live mount, follow a symlink, or rename anything here. The agent can propose; only the frontend can approve.

Three files, one concern each: `rename/plan.rs` is the tool boundary (schema, dispatch, scope + validation + the evidence check), `rename/store.rs` is what a staged proposal IS and how long it lives (rows, display snapshot, accepted-preflight handoff, TTL), `rename/preflight.rs` is user-action-time revalidation (blocks, warnings, fingerprints). Everything the outside world uses is re-exported from `rename/mod.rs`, so callers keep saying `propose::rename::X`.

**Every row must carry verifiable evidence** (`evidence.rs`): a row claiming `imageText` / `imageTags` is refused unless `ImageFactsLedger` has a live delivery **for that path in that chat thread** AND `detail` quotes it. One unbacked row refuses the WHOLE plan; nothing is staged. The ledger fails closed, so don't add a path that trusts a claim when the ledger is missing or empty, and don't widen `EvidenceScope`: evidence never crosses threads. Details: `DETAILS.md`.
