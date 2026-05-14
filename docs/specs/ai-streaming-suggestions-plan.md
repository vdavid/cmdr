# AI streaming for folder suggestions

## Why

Folder name suggestions in `NewFolderDialog` show "Loading..." while the LLM cooks for 1-2 seconds, then all 5 names
appear at once. With streaming, the first suggestion appears in <500 ms and the rest trickle in. The user can act on the
first idea while the LLM is still emitting the rest.

This matters most for two paths:

1. **Local LLM** (3B-param model on Apple Silicon): emits ~30-60 tok/s. The full response takes ~1.5 s of perceived
   waiting; streaming makes it feel instant. This is the path that defines whether Cmdr's "blazing fast" promise holds
   when AI is on.
2. **Cloud AI providers with reasoning models** (`gpt-5*`, `o3*`): time-to-first-output-token can be 1-3 s while the
   model thinks. We can't make thinking faster, but with streaming the user sees the answer as soon as text starts, not
   after the _entire_ answer finishes.

For regular cloud chat models (`gpt-4o-mini`, `claude-haiku`), the wall-clock improvement is smaller (response is
already <1 s) but UX is still noticeably more alive.

This is the literal application of two design principles:

- "Show progress, communicate what's actually happening" (`docs/design-principles.md`)
- "All actions longer than ~1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well" (`docs/design-principles.md`). The cancellation work in this plan is part of the same feature
  precisely because of this rule.

## What we're building

A new Tauri command `stream_folder_suggestions` that:

- Streams sanitized folder name suggestions to the frontend one at a time via `tauri::ipc::Channel<T>`.
- Wraps `genai::Client::exec_chat_stream` under the hood.
- Sanitizes chunk-by-chunk: line-buffers, strips markdown/bullets/numbering, dedupes against existing names.
- **Is properly cancellable**: `cancel_folder_suggestions` Tauri command + `tokio_util::sync::CancellationToken` aborts
  the in-flight `genai` stream and the underlying reqwest connection. Closing the dialog calls cancel.

`NewFolderDialog.svelte` switches from the await-array pattern to subscribing to that channel and rendering names as
they arrive.

The non-streaming `get_folder_suggestions` Tauri command stays as-is. Tests use it. We do **not** mark it deprecated
(per reviewer: "deprecated but used" generates clippy warnings the project rule prohibits).

## What we're NOT building

- **Streaming for `commands/search.rs::translate_search_query`.** The output is a structured key-value response that we
  parse all at once into a `SearchQuery`. There's no progressive UI to show; rendering only happens after parsing
  completes. Streaming would add complexity for zero perceived benefit.
- **Reasoning content / "thinking" chunks** (the `ChatStreamEvent::ReasoningChunk` variant). Reasoning text is
  internal-monologue noise for our use case (we want clean folder names, not "I'm thinking about this..."). We ignore
  those chunks.
- **Tool/function-call chunks.** We don't use tool calling here.
- **Throttling/coalescing on the IPC channel.** Each completed suggestion line is forwarded immediately. Tokens arrive
  at 30-100/s; chunked-line emission is bounded by `max_tokens=150` × ~5-15 lines max. Channel is in-process, unbounded
  (no backpressure concern).
- **A "Started" event with anticipated count.** The model may emit 3 or 7. Faking `expected: 5` would violate radical
  transparency. Frontend shows the trailing-indicator until `Done` arrives. That's enough.
- **Surfacing AI failures to the user via toast or message.** Per the existing graceful-degradation contract (CLAUDE.md
  gotcha: "AI suggestions are a nice-to-have enhancement. Returning empty gracefully hides the failure"), errors stop
  the spinner, log via `log::warn!(target: "ai_suggestions", ...)` for crash bundles, and the section hides if no
  suggestions were collected. No user-facing error string.

## Architecture

### `Channel<T>` over global `app.emit`

The existing AI subsystem uses global Tauri events for download progress (`ai-download-progress`). That's appropriate
for downloads because there's only ever one in flight. Folder suggestions are different:

- The user can open the new-folder dialog, cancel, and reopen quickly. Two streams could overlap if we used a global
  event; listeners from the second open would see chunks from the first.
- A single channel scoped to the call eliminates that race entirely. Tauri 2 docs explicitly recommend `Channel<T>` for
  streaming events from a command.

Tradeoff: it's a new pattern in Cmdr (existing AI events use global emit). Worth it, as call-scoped semantics are the
right fit, and the next streaming AI feature (e.g. future "explain this folder") will reuse the pattern.

### Cancellation: explicit, via `CancellationToken` + companion command

**This is the most important architectural choice in the plan, and it's where the v1 design was wrong.**

`tauri::ipc::Channel<T>::send` is fire-and-forget into Tauri's IPC queue. It does NOT report frontend drops back to the
backend. So "frontend closes dialog → backend's next send fails → backend stops" does NOT work. The backend would keep
streaming until the LLM finishes naturally, billing cloud providers and pegging local CPU/GPU after the user has moved
on.

Per `docs/design-principles.md`: _"All actions longer than ~1 second should be immediately cancelable, canceling not
just the UI but any background processes as well, to avoid wasting the user's resources."_ On `gpt-5-mini` with
reasoning, a folder-suggestion call can take 3+ seconds. Mandatory cancel.

**The model:** the project already does cooperative cancellation in `download.rs` via a `Fn() -> bool` closure. For
streams we use the standard tokio idiom: `tokio_util::sync::CancellationToken`, which interoperates cleanly with
`tokio::select!` and is droppable on `Stream` async iteration.

**Required dependency:** add `tokio-util` to `apps/desktop/src-tauri/Cargo.toml` (place immediately after `futures-util`
on line 76 to keep tokio-stack deps together). Pin to the latest `0.7.x` patch release as of execution date (verify via
`cargo search tokio-util` per the project rule on avoiding 0-day vulns). Use the project's annotation style:

```toml
# tokio-util: provides `CancellationToken` for cooperative cancellation of streaming
# AI suggestions. The `rt` feature enables runtime integration (token wakeups via
# tokio's reactor). MIT/Apache-2.0; tracks tokio's release cadence.
tokio-util = { version = "0.7.X", features = ["rt"] }
```

**The wire:**

- `stream_folder_suggestions` accepts a `request_id: String` (frontend-generated UUID).
- Backend stores `request_id → CancellationToken` in a small `Mutex<HashMap<String, CancellationToken>>` keyed by
  request id, owned by the AI manager.
- Backend spawns a `tokio::task::spawn` task that runs the stream inside `tokio::select!` against the token. When the
  token fires, the future is dropped; `genai::ChatStreamResponse`'s underlying reqwest stream is dropped; the HTTP
  connection closes; billing stops.
- New companion command: `cancel_folder_suggestions(request_id: String)`. It looks up the token and triggers it.
  Idempotent: missing-id → no-op.
- On normal completion (`Done` / `Failed`), the entry is removed from the map by the task itself.

**On the frontend side:**

- `streamFolderSuggestions` generates a UUID, passes it as `requestId`, returns `{ promise, cancel }` where `cancel`
  invokes `cancel_folder_suggestions(requestId)`.
- `NewFolderDialog.svelte` saves the `cancel` function and calls it from `onDestroy`. Idempotent if the stream already
  completed.

**Why not just rely on dialog destruction implicitly?** Because we have no implicit signal. Tauri 2 doesn't tell the
backend "the JS-side Channel was GC'd." We have to send the signal.

**Why not detect window-close at the OS level?** That's a different scope (whole-window). The dialog is a div-modal, not
an OS window. Window-level signals are too coarse.

**Concurrent-streams handling:** if a previous request_id is still active when a new one arrives, we don't auto-cancel
the prior (that would conflate "user reopened the dialog" with "user wants prior request canceled"). Instead, the
dialog's `onDestroy` is what cancels. The new dialog instance starts a fresh request_id. If by chance two stream tasks
coexist briefly (rapid open-close-open), each writes to its own Channel and they don't interfere. The CPU cost is
bounded. A previous local-LLM stream still gets to finish only because its dialog wasn't destroyed first; in practice
if the user closed the prior dialog, `onDestroy` already canceled it.

### Stream-shaped API in `client.rs`, not callback-shaped

`chat_completion_stream` returns a `Stream<Item = Result<String, AiError>>` of content chunks, not a `FnMut` callback.
This matches the natural shape of `genai::Client::exec_chat_stream`, lets the caller drive the loop with normal
`while let Some(chunk) = stream.next().await`, and makes cancellation literally `drop(stream)` (no `should_continue`
flag piped through a closure). Reviewer flagged the closure form as the kind of "hack" the project principles warn
against.

### Line-buffering and sanitization in Rust

AGENTS.md principle: "Smart backend, thin frontend. Complex logic lives in Rust. The frontend's job is to deliver a
delightful UX."

The existing `parse_suggestions` rules (strip markdown/bullets/numbering, length, charset, dedupe case-insensitively
against existing names and already-emitted) are non-trivial. Re-implementing them in TypeScript would create a second
authority. Keep them in Rust; the frontend just renders strings.

The sanitizer is per-line. Existing `parse_suggestions` is refactored to use a new
`sanitize_one_line(raw: &str) -> Option<String>` helper. Both the streaming and non-streaming paths route through that
single function, so they agree by construction.

### Channel payload: discriminated union

```rust
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SuggestionStreamEvent {
    Suggestion { name: String },
    Done,
    Cancelled,
    Failed,
}
```

- `Suggestion { name }`: one fully-sanitized line.
- `Done`: stream completed cleanly. Frontend stops trailing indicator. (Zero `Suggestion`s before `Done` is valid and
  treated the same way the existing flow handles "AI returned empty": section hides if `aiSuggestions.length === 0`.)
- `Cancelled`: backend acknowledges a cancel. Frontend distinguishes "we asked for it" from "it just failed." Logged at
  trace level only.
- `Failed`: backend errored. Frontend stops trailing indicator and treats as graceful degradation. The error string is
  **not** carried in the event; it's logged on the Rust side via `log::warn!(target: "ai_suggestions", ...)` per
  `logging/CLAUDE.md`. Carrying a raw error string we never display would just be a leak.

This shape resolves the reviewer's two concerns: (a) duplicate "errored vs cancelled" ambiguity, (b) leaking error
strings the UI never shows.

### Frontend UX: trailing indicator, not "Loading..." + list

Reviewer correctly flagged that keeping `aiLoading = true` showing "Loading..." text _while_ suggestions are streaming
below it is two contradicting messages. Replace with a subtle pulsing skeleton chip at the end of the list: same height
as a real chip, shows three pulsing dots. Disappears on `Done` / `Cancelled` / `Failed`. No header text duplication.

CSS-only, ~10 lines:

```css
.suggestion-pending {
  /* matches .suggestion-item dimensions */
  animation: pulse 1.2s ease-in-out infinite;
  opacity: 0.5;
  pointer-events: none;
}
@keyframes pulse {
  50% {
    opacity: 0.2;
  }
}
```

The OK button gating is unaffected (`disabled={!isValid || isChecking}` doesn't reference `aiLoading`). Confirm no other
state branches off `aiLoading`.

### Keep both `chat_completion` and `chat_completion_stream`

`commands/search.rs::translate_search_query` parses the entire response before doing anything; streaming gives it
nothing. Forcing it to consume a stream would be ceremony for no benefit. Keep both APIs.

## Implementation

### Step 1: `chat_completion_stream` returns `impl Stream`

**File:** `apps/desktop/src-tauri/src/ai/client.rs`

```rust
use futures_util::stream::BoxStream;

pub async fn chat_completion_stream(
    backend: &AiBackend,
    system_prompt: &str,
    user_prompt: &str,
    options: &ChatOptions,
) -> Result<BoxStream<'static, Result<String, AiError>>, AiError>
```

`BoxStream<'static, Result<String, AiError>>` is just a typed alias for `Pin<Box<dyn Stream + Send + 'static>>`, used to
erase the concrete inner type. Verified: `genai::ChatStream` is already `Send + 'static` (see
`/tmp/rust-genai/src/chat/chat_stream.rs:8`, its inner is `Pin<Box<dyn Stream<...> + Send>>`), and `ChatStreamResponse`
owns its fields, so no `Arc<Client>` lifetime concern.

Import note: the project depends on `futures-util`, not `futures`. Use `futures_util::stream::BoxStream` and
`futures_util::StreamExt` (for `.next()` / `.boxed()`).

Implementation:

1. `resolve_service_target` + `adjust_for_model` (same as non-streaming).
2. `backend.client.exec_chat_stream(...)` → `ChatStreamResponse`.
3. Map the inner stream:
   - `Ok(ChatStreamEvent::Chunk(StreamChunk { content }))` → `Some(Ok(content))`
   - `Ok(ChatStreamEvent::ReasoningChunk(_) | ThoughtSignatureChunk(_) | ToolCallChunk(_))` → skipped (filter step in
     stream)
   - `Ok(ChatStreamEvent::Start | End(_))` → skipped
   - `Err(e)` → `Some(Err(map_genai_error(e)))`, then stream ends
4. `.boxed()`.

The empty-stream case (zero `Chunk` events but clean `End`) emits nothing and ends naturally. Caller treats it as "no
suggestions"; same graceful-degradation as today's "first_text() == None" gotcha (CLAUDE.md).

### Step 2: line-buffer + sanitizer extraction

**File:** `apps/desktop/src-tauri/src/ai/suggestions.rs`

Extract:

```rust
fn sanitize_one_line(raw: &str) -> Option<String>
```

Same rules as today's per-line filter chain in `parse_suggestions`. Returns `None` if the line is invalid (empty,
contains `/`, contains `\0`, len > 255).

Refactor `parse_suggestions` to call it. Behavior unchanged; we just stop having two implementations.

Streaming-state struct:

```rust
struct StreamingSanitizer<'a> {
    existing_names: &'a [String],
    emitted: HashSet<String>,           // case-insensitive
    line_buffer: String,
    suggestions_emitted: usize,
}

impl<'a> StreamingSanitizer<'a> {
    fn new(existing_names: &'a [String]) -> Self;
    fn push_chunk(&mut self, chunk: &str, mut emit: impl FnMut(String) -> bool);
    fn finish(&mut self, emit: impl FnMut(String) -> bool);
}
```

`emit` returns `bool`: `true` = keep going, `false` = caller wants to stop (cancellation, suggestion cap reached). The
sanitizer respects it and stops processing further lines from the buffer.

Stops emitting at `MAX_SUGGESTIONS` (reuse the existing constant in `suggestions.rs`; no new constant).

`finish` flushes the trailing line (LLMs often skip the final `\n`). Has its own dedicated unit test.

### Step 3: Tauri commands `stream_folder_suggestions` + `cancel_folder_suggestions`

**File:** `apps/desktop/src-tauri/src/ai/suggestions.rs`

```rust
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SuggestionStreamEvent {
    Suggestion { name: String },
    Done,
    Cancelled,
    Failed,
}

#[tauri::command]
pub async fn stream_folder_suggestions(
    request_id: String,
    listing_id: String,
    current_path: String,
    include_hidden: bool,
    on_event: tauri::ipc::Channel<SuggestionStreamEvent>,
) -> Result<(), String>

#[tauri::command]
pub fn cancel_folder_suggestions(request_id: String)
```

**Cancellation registry:** lives in the AI manager next to other shared state. Add to `manager.rs` (the existing file
already imports `LazyLock` and `Mutex`; mirror its style):

```rust
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio_util::sync::CancellationToken;

static STREAM_CANCEL_TOKENS: LazyLock<Mutex<HashMap<String, CancellationToken>>>
    = LazyLock::new(Default::default);
```

Helpers:

```rust
pub(super) fn register_stream(request_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    STREAM_CANCEL_TOKENS.lock_ignore_poison().insert(request_id.to_owned(), token.clone());
    token
}
pub(super) fn unregister_stream(request_id: &str) {
    STREAM_CANCEL_TOKENS.lock_ignore_poison().remove(request_id);
}
pub fn cancel_stream(request_id: &str) {
    if let Some(token) = STREAM_CANCEL_TOKENS.lock_ignore_poison().remove(request_id) {
        token.cancel();
    }
}
```

**Use the existing `IgnorePoison` helper** (per `manager.rs` convention): bare `lock()` would panic on poison and
double-panic during the task's panic-unwind. `cancel_stream` removes-and-cancels in one step so a re-arrival
`cancel_folder_suggestions` for the same id is idempotent.

**Stream command flow (no `tokio::spawn`; `#[tauri::command] async fn` already runs on Tauri's async runtime):**

1. **Synchronously register the token** at the very top of the command body, before any `await`. This closes the "cancel
   arrives before token registered" race window.
2. Drop the registration on exit via a small hand-rolled `Drop` newtype guard (`scopeguard` is not a direct project dep,
   no need to add one):

   ```rust
   struct UnregisterGuard<'a>(&'a str);
   impl Drop for UnregisterGuard<'_> {
       fn drop(&mut self) { unregister_stream(self.0); }
   }
   let _guard = UnregisterGuard(&request_id);
   ```

   Drop runs even on panic-unwind, so the registry is always cleaned up.

3. Resolve backend via `manager::resolve_backend()`. On non-`Ready`: send `Done`, return `Ok(())` (graceful, same as
   non-streaming command's contract).
4. Build prompt. Construct `StreamingSanitizer`.
5. Open `chat_completion_stream(...)`. Loop with `tokio::select!` against the token. Required imports:
   `use futures_util::StreamExt;` for `.next()`.

   ```rust
   loop {
       tokio::select! {
           biased;
           _ = token.cancelled() => {
               let _ = on_event.send(SuggestionStreamEvent::Cancelled);
               return Ok(());
           }
           item = stream.next() => match item {
               None => break,
               Some(Ok(chunk)) => sanitizer.push_chunk(&chunk, |name| {
                   // Channel::send returns Err if the webview is gone (window closed
                   // mid-stream). Treat that as implicit cancel: trigger the token so
                   // the next select iteration unwinds cleanly via the cancel arm.
                   match on_event.send(SuggestionStreamEvent::Suggestion { name }) {
                       Ok(()) => true,
                       Err(_) => { token.cancel(); false }
                   }
               }),
               Some(Err(e)) => {
                   log::warn!(target: "ai_suggestions", "stream error: {e}");
                   let _ = on_event.send(SuggestionStreamEvent::Failed);
                   return Ok(());
               }
           }
       }
   }
   sanitizer.finish(|name| on_event.send(SuggestionStreamEvent::Suggestion { name }).is_ok());
   let _ = on_event.send(SuggestionStreamEvent::Done);
   Ok(())
   ```

   On `Cancelled`, the `select!` arm wins and we drop the in-flight `stream.next()` future. The genai stream is then
   dropped, which closes the underlying reqwest body, which cuts off cloud-provider billing and frees local-LLM compute.
   Stream cancel-safety: dropping a not-yet-resolved `next()` future just abandons one poll; for genai's reqwest-backed
   stream this is the desired terminal action.

6. The `Result<(), String>` return is a Tauri requirement; we always `Ok(())`. **Documented decision: this command's IPC
   error channel is intentionally unused; all signaling is via `Channel<SuggestionStreamEvent>`.** Recorded in
   CLAUDE.md.

**Capability file:** verified that `#[tauri::command]` functions registered via `tauri::generate_handler!` do NOT need
explicit entries in `capabilities/default.json` (existing `get_folder_suggestions` works without one). The "missing
permission" gotcha applies to plugin / framework APIs (`setMinSize`, `clipboard:read`, etc.), not to app-defined
commands. **No capability file change needed for the two new commands.** Spelled out here only because round-1 review
flagged it.

### Step 4: Frontend wrapper

**File:** `apps/desktop/src/lib/tauri-commands/settings.ts`

```typescript
export type SuggestionStreamEvent =
  | { type: 'suggestion'; name: string }
  | { type: 'done' }
  | { type: 'cancelled' }
  | { type: 'failed' }

export interface FolderSuggestionsStream {
  promise: Promise<void>
  cancel: () => Promise<void>
}

export function streamFolderSuggestions(
  listingId: string,
  currentPath: string,
  includeHidden: boolean,
  onEvent: (e: SuggestionStreamEvent) => void,
): FolderSuggestionsStream {
  const requestId = crypto.randomUUID()
  const channel = new Channel<SuggestionStreamEvent>()
  channel.onmessage = onEvent
  const promise = invoke<void>('stream_folder_suggestions', {
    requestId,
    listingId,
    currentPath,
    includeHidden,
    onEvent: channel,
  })
  const cancel = async () => {
    try {
      await invoke('cancel_folder_suggestions', { requestId })
    } catch {
      /* idempotent */
    }
  }
  return { promise, cancel }
}
```

UUID via `crypto.randomUUID()` (Web Crypto, available in modern browsers and Tauri's webview).

### Step 5: Update `NewFolderDialog.svelte`

```typescript
let suggestionsStream: FolderSuggestionsStream | undefined
let aiStreaming = $state(false)

async function fetchAiSuggestions() {
  const status = await getAiStatus()
  if (status !== 'available') {
    aiAvailable = false
    return
  }
  aiAvailable = true
  aiSuggestions = []
  aiStreaming = true

  suggestionsStream = streamFolderSuggestions(listingId, currentPath, showHiddenFiles, (event) => {
    switch (event.type) {
      case 'suggestion':
        aiSuggestions = [...aiSuggestions, event.name]
        break
      case 'done':
      case 'cancelled':
      case 'failed':
        aiStreaming = false
        break
    }
  })

  try {
    await suggestionsStream.promise
  } catch {
    aiStreaming = false
  }
}

onDestroy(async () => {
  if (suggestionsStream) await suggestionsStream.cancel()
  // existing cleanup …
})
```

`aiLoading` is replaced by `aiStreaming` (clearer intent). The pulsing chip renders iff `aiStreaming === true`. The
whole `ai-suggestions` container is hidden when `aiAvailable === false` (AI off / not configured); during the brief
`aiAvailable === null` bootstrap window before status is checked, the container shows just its header (no chip, no list
same as today). The "Loading..." span is removed.

Template change (replace the existing `{#if aiAvailable === null || aiLoading}` branch):

```svelte
{#if aiAvailable !== false}
  <div class="ai-suggestions" aria-label="AI suggestions">
    <span class="ai-suggestions-header">AI suggestions:</span>
    {#if aiSuggestions.length > 0}
      <ul role="list" aria-live="polite" aria-relevant="additions">
        {#each aiSuggestions as suggestion (suggestion)}
          <li role="listitem">
            <button class="suggestion-item" onclick={() => selectSuggestion(suggestion)}>{suggestion}</button>
          </li>
        {/each}
        {#if aiStreaming}
          <li role="listitem" aria-hidden="true">
            <span class="suggestion-item suggestion-pending">…</span>
          </li>
        {/if}
      </ul>
    {:else if aiStreaming}
      <span class="suggestion-item suggestion-pending" aria-hidden="true">…</span>
    {/if}
    <!-- if !aiStreaming && length === 0: render nothing (section disappears) -->
  </div>
{/if}
```

`aria-live="polite"` + `aria-relevant="additions"` means screen readers announce each new suggestion as it streams in
without interrupting other speech. The pulsing chip is `aria-hidden` so the ellipsis isn't read out.

Subtle but real UX wins: existing names are immediately clickable while the stream continues; trailing pulse indicates
more is coming; nothing visible when there's nothing to show.

### Step 6: Tests

**Unit (Rust):**

- `sanitize_one_line`: direct from existing `parse_suggestions` test cases.
- `StreamingSanitizer::push_chunk`:
  - chunks split mid-line
  - chunks split exactly on `\n`
  - empty chunks
  - dedupes against existing names (case-insensitive)
  - dedupes against already-emitted (case-insensitive)
  - respects `MAX_SUGGESTIONS` cap
  - per-line markdown / bullet / numbering
  - returns-`false` from `emit` halts further processing in same chunk
- `StreamingSanitizer::finish`: explicit test for trailing-line-without-newline flush.

**Integration (hyper-based mock SSE server, Rust):**

`wiremock` does not chunk-deliver SSE-shaped bodies in distinct frames; it writes the whole body in one HTTP response.
That gives false confidence we'd be exercising multi-chunk parse paths. Replace with a small `hyper::Server` that writes
SSE frames with `tokio::time::sleep` between them. ~30 lines of helper plus tests. `genai`'s own integration tests use
this exact pattern (yakbak); precedent.

New file: `apps/desktop/src-tauri/src/ai/client_streaming_test.rs`: exercises `chat_completion_stream` directly:

- Multi-frame SSE → stream yields each frame's content in order, total assembles to expected text.
- SSE that errors mid-stream → first chunks delivered, then the stream yields `Err(AiError::ServerError(_))`.
- SSE that emits zero `data:` events but ends cleanly → stream yields nothing, ends `Ok`.
- Cancel mid-stream (drop the stream while frames still pending) → server-side request is closed. Track via a
  `oneshot::Sender<()>` that the hyper handler triggers when its body sink errors (client disconnect).

New file: `apps/desktop/src-tauri/src/ai/suggestions_streaming_test.rs`: exercises the Tauri-command layer:

- Two `stream_folder_suggestions` calls with different `request_id`s in flight; cancel one; assert only that one's
  Channel sees `Cancelled`, the other still receives `Suggestion`/`Done`.
- `cancel_folder_suggestions` for an unknown id → no-op (idempotent).

Register both in `apps/desktop/src-tauri/src/ai/mod.rs` next to the existing tests:

```rust
#[cfg(test)]
mod client_streaming_test;
#[cfg(test)]
mod suggestions_streaming_test;
```

**Real-OpenAI smoke (`#[ignore]`-gated, added to existing `client_real_openai_test.rs`):**

- `smoke_gpt_4o_mini_stream` (chat completions SSE)
- `smoke_gpt_5_mini_stream` (Responses API streaming + reasoning empty-result edge)
- `smoke_o3_mini_stream` (chat-completions reasoning model)

**Real-Anthropic smoke (`#[ignore]`-gated, new file `client_real_anthropic_test.rs`):**

Anthropic's native streaming protocol is meaningfully different from OpenAI's SSE shape; without this we'd be testing
only the OpenAI lineage. Mirror the run-command convention in module docs (matches the OpenAI test):

````rust
//! Run with:
//! ```sh
//! ANTHROPIC_API_KEY=$(security find-generic-password -a "$USER" -s "ANTHROPIC_API_KEY" -w) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_anthropic_test
//! ```
````

Tests: `smoke_claude_haiku_stream` against `claude-3-5-haiku-latest`. Add the file to `mod.rs` with the same
`#[cfg(test)] mod client_real_anthropic_test;` pattern.

**Frontend (vitest), `NewFolderDialog.streaming.test.ts`:**

- Channel sends 3 `Suggestion`s then `Done` → DOM shows 3 buttons; pulsing chip disappears.
- Channel sends `Failed` after 2 `Suggestion`s → 2 buttons remain visible; pulsing chip disappears; no toast.
- Channel sends `Cancelled` → same as `Failed` for visual state; logs at trace.
- Channel sends `Done` with zero `Suggestion`s → no buttons, no pulsing chip; section hides.
- Dialog unmounts mid-stream → cancel fn is called once; no console errors.

### Step 7: Docs

`apps/desktop/src-tauri/src/ai/CLAUDE.md`:

- File table: row for `client_streaming_test.rs`; row for `client_real_anthropic_test.rs`; updated `suggestions.rs` row
  mentioning streaming + cancel commands.
- New `Decision/Why`: "Use `Channel<T>` for per-call streaming, not global emit. Why: open-close-open races."
- New `Decision/Why`: "Line-buffering + sanitization in Rust, not frontend. Why: AGENTS.md 'smart backend, thin
  frontend.'"
- New `Decision/Why`: "Streaming command always returns `Ok(())`; signaling is via `Channel<SuggestionStreamEvent>`.
  Why: clearer error contract; one path."
- New `Decision/Why`: "Cancellation via explicit `cancel_folder_suggestions` + `CancellationToken`, not implicit
  Channel-drop detection. Why: Tauri 2 `Channel::send` is fire-and-forget into the IPC queue; backend has no implicit
  drop signal."
- New `Gotcha/Why`: "`Channel::send` succeeds into the void when the JS-side handler is GC'd; `send` only fails if the
  webview itself is gone (window closed). Don't rely on send failure for liveness; use the explicit cancel command.
  Send-error in the streaming suggestion path triggers the token as implicit cancel as a defense-in-depth."
- New `Gotcha/Why`: "Wiremock doesn't chunk-deliver SSE bodies; integration tests use a hand-rolled hyper server."
- New `Gotcha/Why`: "Cancel via `tokio::select!` drops the in-flight `stream.next()` future; for genai's reqwest-backed
  SSE this is the desired terminal action (closes connection, cuts billing). Single-poll cancel-safety is the only model
  we rely on; we never resume a previously-canceled stream."
- Append to the bottom-of-file `Dependencies` section: `tokio-util` (CancellationToken).
- Note next to the cancel-via-explicit-command Decision: "`CancellationToken::cancel` is idempotent; the same token may
  be canceled by an explicit `cancel_folder_suggestions` call AND by an implicit `Channel::send` failure in the same
  tick; both succeed as no-ops after the first."

`docs/architecture.md`: AI subsystem already mapped; add the streaming feature flag/command in the AI section if that
file lists per-subsystem commands. Quick check during implementation.

### Step 8: Run all checks

`./scripts/check.sh` end-to-end. Must pass with the SMB integration test pre-existing failure (Docker not running)
ignored. Real-API smoke tests stay `#[ignore]` so they don't run in default CI.

## Milestones

Sequential, no parallelism needed. Estimate ~half a day:

1. Steps 1-2: client `Stream` API + sanitizer extraction + unit tests (~1.5h)
2. Step 3: Tauri commands + cancellation registry + capability file (~1.5h)
3. Steps 4-5: frontend wrapper + dialog refactor + CSS (~1h)
4. Step 6: hyper SSE harness + integration + smoke tests (~2h, the hyper helper is the largest unknown)
5. Steps 7-8: docs + checks (~30 min)

## Validation

Manual: run `pnpm dev`, open a folder, hit `n` to open the new-folder dialog with each provider type:

- **Off**: dialog shows no suggestion section. (Already works; verify unchanged.)
- **Local LLM**: suggestions stream visibly over 1-2 seconds; trailing chip pulses; chip vanishes on completion.
- **Cloud AI / `gpt-4o-mini`**: suggestions appear ~150 ms apart after a ~500 ms first-token delay.
- **Cloud AI / `gpt-5-mini`** (reasoning): some thinking delay, then a burst of suggestions.
- **Cloud AI / `claude-haiku`** (Anthropic native): smooth stream.

Cancellation: open dialog, watch suggestions start streaming, close dialog before completion. With
`RUST_LOG= cmdr_lib::ai=debug`:

- See `cancel_folder_suggestions` invoked.
- See task drop logged.
- See no further "stream error" logs after cancel.

Open-close-open rapidly 3 times: watch for clean cancel-each-prior; no orphan tasks logged.

## Risks

- **`tauri::ipc::Channel` is new in Cmdr.** Other devs may not know the pattern. Mitigation: documented in CLAUDE.md
  (Step 7).
- **`genai 0.6.0-beta.19` stream API may change before 0.6.0 stable.** We're tracking against beta. Mitigation:
  exact-pinned version; if 0.6.0 stable lands with breaking stream changes, vendor or fork-pin.
- **Hyper SSE harness is ~30 lines we have to maintain.** Acceptable cost. `genai`'s own tests use the same pattern, so
  we're not blazing trail.
- **Cancel race: cancel_folder_suggestions arrives before the stream task has registered its token.** Mitigation: the
  registry is populated synchronously inside the command body before the first `await`, so by the time the command
  returns, the token is registered. Cancel calls received before the command returns are queued by Tauri anyway.
- **Spinner-during-streaming UX**: traded "Loading..." span for a pulsing chip indicator. If the chip is distracting on
  fast cloud paths (<300 ms total), defer to user feedback (easy CSS knob).
