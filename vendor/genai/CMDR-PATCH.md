# genai 0.7.1, patched

The crates.io release of `genai` 0.7.1 (upstream `jeremychone/rust-genai`, published 2026-10-23), swapped in through the
root `Cargo.toml`'s `[patch.crates-io]`. Everything here is the published package byte for byte, apart from the change
below and the files left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.
- `.cargo-ok`, `.github/`, `.gitignore`: registry and upstream tooling, nothing the build reads.
- `tests/`, `examples/`, `dev/`: upstream's integration tests (1.1 MB of recorded provider streams), examples, and
  scripts. The manifest still declares their targets; cargo only reads them when building that target, and a path
  dependency builds the library alone. Two of the library's own unit tests `include_str!` files from `tests/data/`,
  so running this crate's unit tests needs them copied back (below).

## The change

- `src/adapter/adapters/openai/streamer.rs`: a streamed chat-completions answer keeps OpenRouter's
  `delta.reasoning_details` blocks, under the `capture_reasoning_content` opt-in. This file is byte for byte the one on
  the upstream PR branch `openai-stream-reasoning-details` of `vdavid/rust-genai` (commit `7ad48ba`; 0.7.2 left the
  file untouched, so it drops onto 0.7.1 as is). `merge_reasoning_details` follows OpenRouter's own AI SDK rule
  (OpenRouterTeam/ai-sdk-provider#583): a `reasoning.text` or `reasoning.summary` piece extends the last block of the
  same type unless that block is already signed (a signature closes its block), and any other piece
  (`reasoning.encrypted`) is a whole block. ❌ Never merge by `index`: some providers send `index: 0` on every piece,
  which fused two signed Claude thinking blocks into one and broke the signature check on the next turn. At `[DONE]`
  the blocks become `captured_thought_blocks` as `Custom` parts, so `StreamEnd` leads the captured content with them,
  exactly the parts the non-streaming path (`adapter_impl.rs`, upstream PR #301) already makes, and the request side
  already echoes them back as `reasoning_details`. Tests: `test_merge_reasoning_details_*` in the same file, and
  Cmdr's `apps/desktop/src-tauri/src/agent/llm/genai_impl/tests/replay_fixtures.rs`
  (`openrouter_replays_streamed_reasoning_details_verbatim`, `openrouter_keeps_index_zero_signed_reasoning_blocks_apart`).

## Why

OpenRouter carries a reasoning model's continuity in `reasoning_details` (Claude's signed thinking, OpenAI's encrypted
reasoning, Gemini's signatures) and asks for the array back, unmodified and in order, on the next request of a tool
loop. Upstream captures it on the non-streaming path only, and Ask Cmdr streams, so every tool loop through OpenRouter
lost the model's earlier reasoning (verified on upstream `main` at the 2026-10-27 head, by reading
`src/adapter/adapters/openai/streamer.rs`; issue #298 is closed by the non-streaming fix). Streaming shape: OpenRouter's
reasoning-tokens guide, "Streaming Response" (checked 2026-10-27).

The upstream PR is drafted on that branch, not filed yet. Verified live only on an unsigned model
(`nvidia/nemotron-3.5-lightning:free`); a signed model is unverified live, since the OpenRouter key has no credits.

## Running this crate's tests

From a copy outside the repo (the worktree sits inside another workspace, so cargo refuses to test it in place), with
upstream's `Cargo.lock` and `tests/data/openai_generated_*.json` from the crates.io package copied in:
`cargo test --locked --manifest-path <copy>/Cargo.toml --lib streamer`.

## Dropping it

When upstream ships streaming `reasoning_details` capture, delete this folder, its `[patch.crates-io]` line, and its
`exclude` entry, and keep Cmdr's fixture test. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 0.7.1
makes cargo resolve crates.io again.
