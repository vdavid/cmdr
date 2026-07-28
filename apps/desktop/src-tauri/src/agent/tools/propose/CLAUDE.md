# Rename proposals

`rename.rs` stages Ask Cmdr's immutable, in-memory rename plans. It validates only cached pane and index state: never probe a live mount, follow a symlink, or rename anything here. The agent can propose; only the frontend can approve.

**Every row must carry verifiable evidence** (`evidence.rs`): a row claiming `imageText` / `imageTags` is refused unless `ImageFactsLedger` has a live delivery for that path AND `detail` quotes it. One unbacked row refuses the WHOLE plan; nothing is staged. The ledger fails closed, so don't add a path that trusts a claim when the ledger is missing or empty. Details: `DETAILS.md`.
