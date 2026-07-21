# Rename proposals

`rename.rs` stages Ask Cmdr's immutable, in-memory rename plans. It validates only cached pane and index state: never probe a live mount, follow a symlink, or rename anything here. The agent can propose; only the frontend can approve. Details: [DETAILS.md](DETAILS.md).
